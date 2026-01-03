use clap::Parser;

use pmp_deploy::cli::{Cli, Commands, OutputFormat, ProvisionArgs};

#[test]
fn test_provision_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "provision", "production"]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "production");
            assert!(!args.plan);
            assert!(!args.dry_run);
            assert!(!args.yes);
            assert!(args.image.is_none());
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_plan_flag() {
    let cli = Cli::parse_from(["pmp-deploy", "provision", "staging", "--plan"]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "staging");
            assert!(args.plan);
            assert!(!args.dry_run);
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_dry_run_flag() {
    let cli = Cli::parse_from(["pmp-deploy", "provision", "dev", "--dry-run"]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "dev");
            assert!(!args.plan);
            assert!(args.dry_run);
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_yes_flag() {
    let cli = Cli::parse_from(["pmp-deploy", "provision", "production", "-y"]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert!(args.yes);
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_yes_long_flag() {
    let cli = Cli::parse_from(["pmp-deploy", "provision", "production", "--yes"]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert!(args.yes);
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_image_override() {
    let cli = Cli::parse_from([
        "pmp-deploy",
        "provision",
        "production",
        "--image",
        "myapp:v2.0.0",
    ]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.image, Some("myapp:v2.0.0".to_string()));
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_all_flags() {
    let cli = Cli::parse_from([
        "pmp-deploy",
        "provision",
        "production",
        "--plan",
        "--dry-run",
        "-y",
        "--image",
        "myapp:latest",
    ]);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "production");
            assert!(args.plan);
            assert!(args.dry_run);
            assert!(args.yes);
            assert_eq!(args.image, Some("myapp:latest".to_string()));
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_global_verbose() {
    let cli = Cli::parse_from(["pmp-deploy", "-v", "provision", "production"]);

    assert!(cli.verbose);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "production");
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_global_quiet() {
    let cli = Cli::parse_from(["pmp-deploy", "-q", "provision", "staging"]);

    assert!(cli.quiet);

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "staging");
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_config_file() {
    let cli = Cli::parse_from([
        "pmp-deploy",
        "-f",
        "custom-config.yaml",
        "provision",
        "production",
    ]);

    assert_eq!(
        cli.file,
        Some(std::path::PathBuf::from("custom-config.yaml"))
    );

    match cli.command {
        Some(Commands::Provision(args)) => {
            assert_eq!(args.environment, "production");
        }
        _ => panic!("Expected Provision command"),
    }
}

#[test]
fn test_provision_command_with_json_output() {
    let cli = Cli::parse_from(["pmp-deploy", "--output", "json", "provision", "dev"]);

    assert!(matches!(cli.output, OutputFormat::Json));
}

#[test]
fn test_provision_args_default_values() {
    let args = ProvisionArgs {
        environment: "test".to_string(),
        plan: false,
        dry_run: false,
        yes: false,
        image: None,
    };

    assert_eq!(args.environment, "test");
    assert!(!args.plan);
    assert!(!args.dry_run);
    assert!(!args.yes);
    assert!(args.image.is_none());
}

#[test]
fn test_deploy_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "deploy", "production"]);

    match cli.command {
        Some(Commands::Deploy(args)) => {
            assert_eq!(args.environment, "production");
            assert!(!args.dry_run);
            assert!(!args.yes);
            assert!(args.image.is_none());
            assert_eq!(args.deploy_mode, "app-only");
        }
        _ => panic!("Expected Deploy command"),
    }
}

#[test]
fn test_deploy_command_with_full_mode() {
    let cli = Cli::parse_from([
        "pmp-deploy",
        "deploy",
        "production",
        "--deploy-mode",
        "full",
    ]);

    match cli.command {
        Some(Commands::Deploy(args)) => {
            assert_eq!(args.deploy_mode, "full");
        }
        _ => panic!("Expected Deploy command"),
    }
}

#[test]
fn test_status_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "status", "production"]);

    match cli.command {
        Some(Commands::Status(args)) => {
            assert_eq!(args.environment, "production");
            assert!(!args.watch);
        }
        _ => panic!("Expected Status command"),
    }
}

#[test]
fn test_status_command_with_watch() {
    let cli = Cli::parse_from(["pmp-deploy", "status", "production", "-w"]);

    match cli.command {
        Some(Commands::Status(args)) => {
            assert!(args.watch);
        }
        _ => panic!("Expected Status command"),
    }
}

#[test]
fn test_rollback_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "rollback", "production"]);

    match cli.command {
        Some(Commands::Rollback(args)) => {
            assert_eq!(args.environment, "production");
            assert!(args.target_version.is_none());
            assert!(!args.yes);
        }
        _ => panic!("Expected Rollback command"),
    }
}

#[test]
fn test_rollback_command_with_version() {
    let cli = Cli::parse_from([
        "pmp-deploy",
        "rollback",
        "production",
        "--to-version",
        "v1.2.3",
    ]);

    match cli.command {
        Some(Commands::Rollback(args)) => {
            assert_eq!(args.target_version, Some("v1.2.3".to_string()));
        }
        _ => panic!("Expected Rollback command"),
    }
}

#[test]
fn test_logs_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "logs", "production"]);

    match cli.command {
        Some(Commands::Logs(args)) => {
            assert_eq!(args.environment, "production");
            assert!(!args.follow);
            assert_eq!(args.lines, 100);
        }
        _ => panic!("Expected Logs command"),
    }
}

#[test]
fn test_logs_command_with_follow() {
    let cli = Cli::parse_from(["pmp-deploy", "logs", "production", "-F"]);

    match cli.command {
        Some(Commands::Logs(args)) => {
            assert!(args.follow);
        }
        _ => panic!("Expected Logs command"),
    }
}

#[test]
fn test_logs_command_with_lines() {
    let cli = Cli::parse_from(["pmp-deploy", "logs", "production", "-n", "500"]);

    match cli.command {
        Some(Commands::Logs(args)) => {
            assert_eq!(args.lines, 500);
        }
        _ => panic!("Expected Logs command"),
    }
}

#[test]
fn test_list_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "list"]);

    assert!(matches!(cli.command, Some(Commands::List)));
}

#[test]
fn test_validate_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "validate"]);

    assert!(matches!(cli.command, Some(Commands::Validate)));
}

#[test]
fn test_init_command_parsing() {
    let cli = Cli::parse_from(["pmp-deploy", "init"]);

    match cli.command {
        Some(Commands::Init(args)) => {
            assert!(args.infrastructure.is_none());
            assert!(!args.force);
        }
        _ => panic!("Expected Init command"),
    }
}

#[test]
fn test_init_command_with_infrastructure() {
    let cli = Cli::parse_from(["pmp-deploy", "init", "--infrastructure", "aws-ecs"]);

    match cli.command {
        Some(Commands::Init(args)) => {
            assert_eq!(args.infrastructure, Some("aws-ecs".to_string()));
        }
        _ => panic!("Expected Init command"),
    }
}

#[test]
fn test_init_command_with_force() {
    let cli = Cli::parse_from(["pmp-deploy", "init", "--force"]);

    match cli.command {
        Some(Commands::Init(args)) => {
            assert!(args.force);
        }
        _ => panic!("Expected Init command"),
    }
}

#[test]
fn test_ui_command_default_values() {
    let cli = Cli::parse_from(["pmp-deploy", "ui"]);

    match cli.command {
        Some(Commands::Ui(args)) => {
            assert_eq!(args.port, 8080);
            assert_eq!(args.host, "127.0.0.1");
            assert!(!args.open);
            assert!(args.tls_cert.is_none());
            assert!(args.tls_key.is_none());
            assert!(!args.dev_tls);
        }
        _ => panic!("Expected Ui command"),
    }
}

#[test]
fn test_ui_command_with_port() {
    let cli = Cli::parse_from(["pmp-deploy", "ui", "-p", "3000"]);

    match cli.command {
        Some(Commands::Ui(args)) => {
            assert_eq!(args.port, 3000);
        }
        _ => panic!("Expected Ui command"),
    }
}

#[test]
fn test_ui_command_with_host() {
    let cli = Cli::parse_from(["pmp-deploy", "ui", "--host", "0.0.0.0"]);

    match cli.command {
        Some(Commands::Ui(args)) => {
            assert_eq!(args.host, "0.0.0.0");
        }
        _ => panic!("Expected Ui command"),
    }
}

#[test]
fn test_ui_command_with_open() {
    let cli = Cli::parse_from(["pmp-deploy", "ui", "--open"]);

    match cli.command {
        Some(Commands::Ui(args)) => {
            assert!(args.open);
        }
        _ => panic!("Expected Ui command"),
    }
}

#[test]
fn test_ui_command_with_dev_tls() {
    let cli = Cli::parse_from(["pmp-deploy", "ui", "--dev-tls"]);

    match cli.command {
        Some(Commands::Ui(args)) => {
            assert!(args.dev_tls);
        }
        _ => panic!("Expected Ui command"),
    }
}

#[test]
fn test_output_format_default() {
    let cli = Cli::parse_from(["pmp-deploy", "list"]);

    assert!(matches!(cli.output, OutputFormat::Text));
}

#[test]
fn test_no_command_provided() {
    let cli = Cli::parse_from(["pmp-deploy"]);

    assert!(cli.command.is_none());
}
