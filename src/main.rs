use clap::Parser;
use dialoguer::{Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use pmp_deploy::cli::{Cli, Commands, HooksCommands, OutputFormat, ProjectsCommands};
use pmp_deploy::config::{ConfigLoader, ConfigValidator, EnvironmentConfig};
use pmp_deploy::infrastructure::{DeployMode, DeploymentContext, ProviderFactory};
use pmp_deploy::ui::{start_server, start_server_https, generate_dev_certificate};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    init_logging(&cli);

    let Cli {
        command,
        file,
        verbose,
        quiet,
        output,
    } = cli;

    let cli_context = CliContext {
        file,
        verbose,
        quiet,
        output,
    };

    match command {
        Some(cmd) => run_command(cmd, cli_context).await,
        None => run_interactive(cli_context).await,
    }
}

#[derive(Debug)]
struct CliContext {
    file: Option<std::path::PathBuf>,
    verbose: bool,
    quiet: bool,
    output: OutputFormat,
}

fn init_logging(cli: &Cli) {
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else if cli.quiet {
        EnvFilter::new("error")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

async fn run_command(command: Commands, ctx: CliContext) -> anyhow::Result<()> {
    match command {
        Commands::Deploy(args) => cmd_deploy(args, &ctx).await,
        Commands::Status(args) => cmd_status(args, &ctx).await,
        Commands::Rollback(args) => cmd_rollback(args, &ctx).await,
        Commands::Logs(args) => cmd_logs(args, &ctx).await,
        Commands::List => cmd_list(&ctx).await,
        Commands::Validate => cmd_validate(&ctx).await,
        Commands::Init(args) => cmd_init(args, &ctx).await,
        Commands::Provision(args) => cmd_provision(args, &ctx).await,
        Commands::Projects(subcmd) => cmd_projects(subcmd, &ctx).await,
        Commands::Hooks(subcmd) => cmd_hooks(subcmd, &ctx).await,
        Commands::Ui(args) => cmd_ui(args).await,
    }
}

async fn run_interactive(ctx: CliContext) -> anyhow::Result<()> {
    let loader = ConfigLoader::new();

    if loader.project_config_exists() {
        let config = loader.load_project_config()?;
        let environments: Vec<&str> = config.list_environments();

        if environments.is_empty() {
            println!("No environments configured. Run 'pmp-deploy init' to get started.");
            return Ok(());
        }

        let selection = Select::new()
            .with_prompt("Select an environment to deploy")
            .items(&environments)
            .default(0)
            .interact()?;

        let env_name = environments[selection];
        println!("Selected: {}", env_name);

        let args = pmp_deploy::cli::DeployArgs {
            environment: env_name.to_string(),
            dry_run: false,
            yes: false,
            image: None,
            deploy_mode: "app-only".to_string(),
            skip_hooks: false,
            skip_pre_hooks: false,
            skip_post_hooks: false,
        };

        cmd_deploy(args, &ctx).await
    } else {
        let global_config = loader.load_global_config()?;

        if let Some(global) = global_config {
            if global.projects.is_empty() {
                println!("No projects configured. Add projects with 'pmp-deploy projects add <path>'");
                return Ok(());
            }

            let project_names: Vec<String> =
                global.projects.iter().map(|p| p.display_name()).collect();

            let selection = Select::new()
                .with_prompt("Select a project")
                .items(&project_names)
                .default(0)
                .interact()?;

            let project = &global.projects[selection];
            println!(
                "Selected project: {} ({})",
                project.display_name(),
                project.expanded_path().display()
            );

            println!(
                "Run 'pmp-deploy --file {}/.pmp-deploy.yaml' to work with this project",
                project.expanded_path().display()
            );
        } else {
            println!("No configuration found.");
            println!("Run 'pmp-deploy init' to create a new config, or");
            println!("Run 'pmp-deploy projects add <path>' to add existing projects.");
        }

        Ok(())
    }
}

async fn cmd_deploy(
    args: pmp_deploy::cli::DeployArgs,
    ctx: &CliContext,
) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&args.environment)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| {
            anyhow::anyhow!("Infrastructure '{}' not found", env.infrastructure)
        })?;

    // Override image if provided via CLI
    let mut env_config = env.clone();

    if let Some(image) = args.image.as_ref() {
        env_config.image = Some(image.clone());
    }

    let deploy_mode = DeployMode::from_str(&args.deploy_mode);

    if args.dry_run {
        let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
        let deploy_ctx = create_deployment_context(&args.environment, &env_config, true, ctx.verbose, deploy_mode);
        let result = provider.deploy(&deploy_ctx).await?;

        println!("DRY RUN: {}", result.message);
        return Ok(());
    }

    if !args.yes {
        let confirmed = Confirm::new()
            .with_prompt(format!("Deploy to '{}'?", args.environment))
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Deployment cancelled.");
            return Ok(());
        }
    }

    let progress = create_progress_bar("Deploying...");

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&args.environment, &env_config, false, ctx.verbose, deploy_mode);

    let result = provider.deploy(&deploy_ctx).await?;

    progress.finish_with_message("Done!");

    if result.success {
        println!("Deployment successful: {}", result.message);

        if let Some(version) = result.version {
            println!("  Version: {}", version);
        }
    } else {
        eprintln!("Deployment failed: {}", result.message);
    }

    Ok(())
}

async fn cmd_status(
    args: pmp_deploy::cli::StatusArgs,
    ctx: &CliContext,
) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&args.environment)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| {
            anyhow::anyhow!("Infrastructure '{}' not found", env.infrastructure)
        })?;

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&args.environment, env, false, ctx.verbose, DeployMode::Full);

    let status = provider.status(&deploy_ctx).await?;

    match ctx.output {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "environment": args.environment,
                "status": status
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Text => {
            println!("Status for '{}':", args.environment);
            println!("{}", status);
        }
    }

    Ok(())
}

async fn cmd_rollback(
    args: pmp_deploy::cli::RollbackArgs,
    ctx: &CliContext,
) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&args.environment)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| {
            anyhow::anyhow!("Infrastructure '{}' not found", env.infrastructure)
        })?;

    if !args.yes {
        let confirmed = Confirm::new()
            .with_prompt(format!("Rollback '{}'?", args.environment))
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Rollback cancelled.");
            return Ok(());
        }
    }

    let progress = create_progress_bar("Rolling back...");

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&args.environment, env, false, ctx.verbose, DeployMode::Full);

    let result = provider.rollback(&deploy_ctx).await?;

    progress.finish_with_message("Done!");

    if result.success {
        println!("Rollback successful: {}", result.message);
    } else {
        eprintln!("Rollback failed: {}", result.message);
    }

    Ok(())
}

async fn cmd_logs(
    args: pmp_deploy::cli::LogsArgs,
    ctx: &CliContext,
) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&args.environment)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| {
            anyhow::anyhow!("Infrastructure '{}' not found", env.infrastructure)
        })?;

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&args.environment, env, false, ctx.verbose, DeployMode::Full);

    println!("Fetching logs for '{}'...", args.environment);
    provider.logs(&deploy_ctx, args.follow).await?;

    Ok(())
}

async fn cmd_list(ctx: &CliContext) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let environments = config.list_environments();

    match ctx.output {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "environments": environments
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Text => {
            println!("Available environments:");

            for env_name in environments {
                let env = config.get_environment(env_name).unwrap();
                let infra = config.get_infrastructure(&env.infrastructure);
                let infra_type = infra
                    .map(|i| i.infrastructure_type.as_str())
                    .unwrap_or("unknown");

                println!(
                    "  {} -> {} ({})",
                    env_name, env.infrastructure, infra_type
                );
            }
        }
    }

    Ok(())
}

async fn cmd_validate(ctx: &CliContext) -> anyhow::Result<()> {
    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    match ConfigValidator::validate(&config) {
        Ok(()) => {
            println!("Configuration is valid.");
            Ok(())
        }
        Err(errors) => {
            eprintln!("Configuration validation failed:");

            for error in &errors {
                eprintln!("  - {}", error);
            }

            anyhow::bail!("{} validation error(s) found", errors.len())
        }
    }
}

async fn cmd_provision(
    args: pmp_deploy::cli::ProvisionArgs,
    ctx: &CliContext,
) -> anyhow::Result<()> {
    use pmp_deploy::infrastructure::{
        InfrastructureType, PlannedAction, ProvisioningAction,
    };

    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&args.environment)
        .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| {
            anyhow::anyhow!("Infrastructure '{}' not found", env.infrastructure)
        })?;

    // Get infrastructure type
    let infra_type = InfrastructureType::from_str(&infra.infrastructure_type);

    // Override image if provided via CLI
    let image = args
        .image
        .as_ref()
        .or(env.image.as_ref())
        .ok_or_else(|| anyhow::anyhow!("No image specified (use --image or set in config)"))?;

    // Check if provisioning is configured
    if infra.config.get("provision").is_none() {
        anyhow::bail!(
            "No provisioning configuration found for infrastructure '{}'.\n\
             Add a 'provision' block to your infrastructure config.",
            env.infrastructure
        );
    }

    match infra_type {
        InfrastructureType::AwsLambda => {
            let provider =
                pmp_deploy::infrastructure::aws_lambda::AwsLambdaProvider::from_config(infra)
                    .await?;

            if args.plan || args.dry_run {
                let plan = provider.plan_provisioning().await?;

                println!("Provisioning plan for '{}':", args.environment);
                println!();

                if plan.is_empty() {
                    println!("  No changes needed.");
                } else {
                    for change in &plan.changes {
                        let action_str = match change.action {
                            PlannedAction::Create => "+",
                            PlannedAction::Update => "~",
                            PlannedAction::NoChange => " ",
                        };

                        println!(
                            "  {} {} ({}): {}",
                            action_str, change.resource_type, change.resource_name, change.reason
                        );

                        if let (Some(current), Some(desired)) =
                            (&change.current, &change.desired)
                        {
                            println!("      {} -> {}", current, desired);
                        }
                    }
                }

                return Ok(());
            }

            if !args.yes {
                let confirmed = Confirm::new()
                    .with_prompt(format!(
                        "Provision Lambda infrastructure for '{}'?",
                        args.environment
                    ))
                    .default(false)
                    .interact()?;

                if !confirmed {
                    println!("Provisioning cancelled.");
                    return Ok(());
                }
            }

            let progress = create_progress_bar("Provisioning Lambda...");

            let result = provider.provision(image).await?;

            progress.finish_with_message("Done!");

            let action_str = match result.action {
                ProvisioningAction::Created => "Created",
                ProvisioningAction::Updated => "Updated",
                ProvisioningAction::Unchanged => "Unchanged",
            };

            println!(
                "{} {} '{}'",
                action_str, result.resource_type, result.resource_name
            );

            for detail in &result.details {
                println!("  {}", detail);
            }
        }

        InfrastructureType::AwsEcs => {
            let provider =
                pmp_deploy::infrastructure::aws_ecs::AwsEcsProvider::from_config(infra).await?;

            let service_name = infra
                .config
                .get("service_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("service_name is required for ECS provisioning"))?;

            if args.plan || args.dry_run {
                let plan = provider.plan_provisioning(service_name).await?;

                println!("Provisioning plan for '{}':", args.environment);
                println!();

                if plan.is_empty() {
                    println!("  No changes needed.");
                } else {
                    for change in &plan.changes {
                        let action_str = match change.action {
                            PlannedAction::Create => "+",
                            PlannedAction::Update => "~",
                            PlannedAction::NoChange => " ",
                        };

                        println!(
                            "  {} {} ({}): {}",
                            action_str, change.resource_type, change.resource_name, change.reason
                        );

                        if let (Some(current), Some(desired)) =
                            (&change.current, &change.desired)
                        {
                            println!("      {} -> {}", current, desired);
                        }
                    }
                }

                return Ok(());
            }

            if !args.yes {
                let confirmed = Confirm::new()
                    .with_prompt(format!(
                        "Provision ECS infrastructure for '{}'?",
                        args.environment
                    ))
                    .default(false)
                    .interact()?;

                if !confirmed {
                    println!("Provisioning cancelled.");
                    return Ok(());
                }
            }

            let progress = create_progress_bar("Provisioning ECS...");

            let results = provider.provision(service_name, image).await?;

            progress.finish_with_message("Done!");

            for result in results {
                let action_str = match result.action {
                    ProvisioningAction::Created => "Created",
                    ProvisioningAction::Updated => "Updated",
                    ProvisioningAction::Unchanged => "Unchanged",
                };

                println!(
                    "{} {} '{}'",
                    action_str, result.resource_type, result.resource_name
                );

                for detail in &result.details {
                    println!("  {}", detail);
                }
            }
        }

        InfrastructureType::Kubernetes | InfrastructureType::AwsEks => {
            // K8s/EKS provisioning is handled by Helm's upgrade --install
            println!(
                "Kubernetes infrastructure provisioning is handled automatically by Helm.\n\
                 Use 'pmp-deploy deploy {}' with Helm configuration instead.",
                args.environment
            );
        }

        _ => {
            anyhow::bail!(
                "Provisioning not supported for infrastructure type '{}'",
                infra.infrastructure_type
            );
        }
    }

    Ok(())
}

async fn cmd_init(
    args: pmp_deploy::cli::InitArgs,
    _ctx: &CliContext,
) -> anyhow::Result<()> {
    let config_path = std::env::current_dir()?.join(".pmp-deploy.yaml");

    if config_path.exists() && !args.force {
        anyhow::bail!(
            "Configuration file already exists. Use --force to overwrite."
        );
    }

    let infra_type = args.infrastructure.as_deref().unwrap_or("docker-compose");

    let template = generate_config_template(infra_type);

    std::fs::write(&config_path, template)?;
    println!("Created configuration file: {}", config_path.display());

    Ok(())
}

fn generate_config_template(infra_type: &str) -> String {
    match infra_type {
        "aws-eks" => r#"# pmp-deploy configuration
infrastructure:
  aws-dev:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1
      namespace: default

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:latest
    replicas: 2
"#
        .to_string(),

        "aws-ecs" => r#"# pmp-deploy configuration
infrastructure:
  aws-dev:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      launch_type: FARGATE

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:latest
"#
        .to_string(),

        "kubernetes" => r#"# pmp-deploy configuration
infrastructure:
  k8s-local:
    type: kubernetes
    config:
      context: my-context
      namespace: default

environments:
  development:
    infrastructure: k8s-local
    deployment_type: rolling-update
    image: my-app:latest
    replicas: 2
"#
        .to_string(),

        _ => r#"# pmp-deploy configuration
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml
      project_name: my-app

environments:
  development:
    infrastructure: local
    deployment_type: all-in
    image: my-app:latest
"#
        .to_string(),
    }
}

async fn cmd_projects(subcmd: ProjectsCommands, _ctx: &CliContext) -> anyhow::Result<()> {
    match subcmd {
        ProjectsCommands::List => {
            let loader = ConfigLoader::new();
            let global_config = loader.load_global_config()?;

            if let Some(global) = global_config {
                if global.projects.is_empty() {
                    println!("No projects configured.");
                } else {
                    println!("Configured projects:");

                    for project in &global.projects {
                        let path = project.expanded_path();
                        let exists = path.join(".pmp-deploy.yaml").exists();
                        let status = if exists { "OK" } else { "NOT FOUND" };

                        println!(
                            "  {} -> {} [{}]",
                            project.display_name(),
                            path.display(),
                            status
                        );
                    }
                }
            } else {
                println!("No global configuration found at ~/.pmp-deploy.yaml");
            }

            Ok(())
        }

        ProjectsCommands::Add(args) => {
            // TODO: Implement project add
            println!(
                "Adding project: {} (name: {:?})",
                args.path.display(),
                args.name
            );
            println!("Project management will be fully implemented in Milestone 6");

            Ok(())
        }

        ProjectsCommands::Remove(args) => {
            // TODO: Implement project remove
            println!("Removing project: {}", args.project);
            println!("Project management will be fully implemented in Milestone 6");

            Ok(())
        }
    }
}

async fn cmd_hooks(subcmd: HooksCommands, ctx: &CliContext) -> anyhow::Result<()> {
    use pmp_deploy::hooks::{
        ContainerHookExecutor, EcsTaskHookExecutor, HookRunner, HttpHookExecutor,
        K8sJobHookExecutor, LambdaHookExecutor,
    };
    use pmp_deploy::hooks::executor::HookContext;

    let loader = create_config_loader(ctx);
    let config = loader.load_project_config()?;

    match subcmd {
        HooksCommands::Run(args) => {
            let env = config
                .get_environment(&args.environment)
                .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

            let hooks_config = env.hooks.as_ref().ok_or_else(|| {
                anyhow::anyhow!("No hooks configured for environment '{}'", args.environment)
            })?;

            let infra = config.get_infrastructure(&env.infrastructure);
            let infra_type = infra
                .map(|i| i.infrastructure_type.as_str())
                .unwrap_or("unknown");

            // Create hook runner with all executors
            let mut runner = HookRunner::new();
            runner.register(ContainerHookExecutor::new());
            runner.register(HttpHookExecutor::new());
            runner.register(EcsTaskHookExecutor::new());
            runner.register(K8sJobHookExecutor::new());
            runner.register(LambdaHookExecutor::new());

            // Create hook context
            let hook_ctx = HookContext::new(&args.environment, infra_type)
                .with_dry_run(args.dry_run);

            println!("Running hook '{}' in environment '{}'...", args.hook_name, args.environment);

            let result = runner
                .run_hook_by_name(hooks_config, &args.hook_name, &hook_ctx)
                .await?;

            if result.success {
                println!("Hook '{}' completed successfully in {:?}", result.name, result.duration);

                if !result.output.is_empty() {
                    println!("\nOutput:\n{}", result.output);
                }
            } else {
                eprintln!("Hook '{}' failed: {}", result.name, result.error.unwrap_or_default());
            }

            Ok(())
        }

        HooksCommands::List(args) => {
            let env = config
                .get_environment(&args.environment)
                .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", args.environment))?;

            let hooks_config = env.hooks.as_ref();

            match ctx.output {
                OutputFormat::Json => {
                    let json = serde_json::json!({
                        "environment": args.environment,
                        "hooks": hooks_config
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                OutputFormat::Text => {
                    println!("Hooks for '{}':", args.environment);

                    if let Some(hooks) = hooks_config {
                        if !hooks.pre_deploy.is_empty() {
                            println!("\n  Pre-deploy hooks:");

                            for hook in &hooks.pre_deploy {
                                println!(
                                    "    - {} ({}, timeout: {}s, fail_on_error: {})",
                                    hook.name,
                                    hook.hook_type.as_str(),
                                    hook.timeout_secs,
                                    hook.fail_on_error
                                );
                            }
                        }

                        if !hooks.post_deploy.is_empty() {
                            println!("\n  Post-deploy hooks:");

                            for hook in &hooks.post_deploy {
                                println!(
                                    "    - {} ({}, timeout: {}s, fail_on_error: {})",
                                    hook.name,
                                    hook.hook_type.as_str(),
                                    hook.timeout_secs,
                                    hook.fail_on_error
                                );
                            }
                        }

                        if !hooks.on_failure.is_empty() {
                            println!("\n  On-failure hooks:");

                            for hook in &hooks.on_failure {
                                println!(
                                    "    - {} ({}, timeout: {}s)",
                                    hook.name,
                                    hook.hook_type.as_str(),
                                    hook.timeout_secs
                                );
                            }
                        }

                        if hooks.is_empty() {
                            println!("  No hooks configured.");
                        }
                    } else {
                        println!("  No hooks configured.");
                    }
                }
            }

            Ok(())
        }
    }
}

async fn cmd_ui(args: pmp_deploy::cli::UiArgs) -> anyhow::Result<()> {
    let scheme = if args.tls_cert.is_some() || args.dev_tls { "https" } else { "http" };
    println!("Starting UI server at {}://{}:{}", scheme, args.host, args.port);

    if args.open {
        let url = format!("{}://{}:{}", scheme, args.host, args.port);

        if let Err(e) = open::that(&url) {
            tracing::warn!("Failed to open browser: {}", e);
        }
    }

    if args.dev_tls {
        let cert_dir = directories::BaseDirs::new()
            .map(|d| d.data_dir().join("pmp-deploy").join("certs"))
            .unwrap_or_else(|| std::path::PathBuf::from(".pmp-deploy/certs"));

        std::fs::create_dir_all(&cert_dir)?;

        let (cert_path, key_path) = generate_dev_certificate(&cert_dir)?;
        start_server_https(
            &args.host,
            args.port,
            cert_path.to_str().unwrap(),
            key_path.to_str().unwrap(),
        ).await
    } else if let (Some(cert), Some(key)) = (args.tls_cert, args.tls_key) {
        start_server_https(
            &args.host,
            args.port,
            cert.to_str().unwrap(),
            key.to_str().unwrap(),
        ).await
    } else {
        start_server(&args.host, args.port).await
    }
}

fn create_config_loader(ctx: &CliContext) -> ConfigLoader {
    let mut loader = ConfigLoader::new();

    if let Some(path) = &ctx.file {
        loader = loader.with_project_config(path.clone());
    }

    loader
}

fn create_deployment_context(
    name: &str,
    env: &EnvironmentConfig,
    dry_run: bool,
    verbose: bool,
    deploy_mode: DeployMode,
) -> DeploymentContext {
    DeploymentContext {
        environment_name: name.to_string(),
        environment: env.clone(),
        dry_run,
        verbose,
        deploy_mode,
    }
}

fn create_progress_bar(message: &str) -> ProgressBar {
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    progress.set_message(message.to_string());
    progress.enable_steady_tick(std::time::Duration::from_millis(100));
    progress
}
