//! Extended AWS Lambda features: ZIP packaging, layer management, and event source mappings.
//!
//! This module provides additional Lambda deployment capabilities beyond container images.

use aws_sdk_lambda::types::{FilterCriteria, SourceAccessConfiguration, SourceAccessType};
use aws_sdk_lambda::Client as LambdaClient;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
use std::path::Path;

// ============================================================================
// ZIP Packaging
// ============================================================================

/// Configuration for ZIP-based Lambda deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipPackageConfig {
    /// Path to the source directory or file to package.
    pub source_path: String,

    /// S3 bucket for uploading the ZIP file (optional).
    /// If not specified, the ZIP is deployed directly (limited to 50MB).
    pub s3_bucket: Option<String>,

    /// S3 key prefix for the ZIP file.
    #[serde(default)]
    pub s3_prefix: Option<String>,

    /// Files/patterns to exclude from the ZIP.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Whether to include hidden files.
    #[serde(default)]
    pub include_hidden: bool,
}

impl ZipPackageConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    /// Get the S3 key for the ZIP file.
    pub fn s3_key(&self, function_name: &str) -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(prefix) = &self.s3_prefix {
            format!("{}/{}-{}.zip", prefix, function_name, timestamp)
        } else {
            format!("{}-{}.zip", function_name, timestamp)
        }
    }
}

/// Packages Lambda code into a ZIP file.
pub struct LambdaPackager;

impl LambdaPackager {
    /// Create a ZIP file from the source path.
    pub fn create_zip(config: &ZipPackageConfig) -> anyhow::Result<Vec<u8>> {
        let source_path = Path::new(&config.source_path);

        if !source_path.exists() {
            anyhow::bail!("Source path does not exist: {}", config.source_path);
        }

        let mut buffer = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buffer);
            let mut zip = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            if source_path.is_file() {
                Self::add_file_to_zip(&mut zip, source_path, source_path, &options)?;
            } else {
                Self::add_directory_to_zip(&mut zip, source_path, source_path, config, &options)?;
            }

            zip.finish()?;
        }

        tracing::info!(
            "Created ZIP package: {} bytes from {}",
            buffer.len(),
            config.source_path
        );

        Ok(buffer)
    }

    fn add_file_to_zip<W: Write + Seek>(
        zip: &mut zip::ZipWriter<W>,
        file_path: &Path,
        base_path: &Path,
        options: &zip::write::SimpleFileOptions,
    ) -> anyhow::Result<()> {
        let name = file_path
            .strip_prefix(base_path)
            .unwrap_or(file_path)
            .to_string_lossy();

        let name = if name.is_empty() {
            file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string())
        } else {
            name.to_string()
        };

        zip.start_file(&name, *options)?;

        let mut file = std::fs::File::open(file_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        zip.write_all(&contents)?;

        Ok(())
    }

    fn add_directory_to_zip<W: Write + Seek>(
        zip: &mut zip::ZipWriter<W>,
        dir_path: &Path,
        base_path: &Path,
        config: &ZipPackageConfig,
        options: &zip::write::SimpleFileOptions,
    ) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files unless configured to include them
            if !config.include_hidden && file_name.starts_with('.') {
                continue;
            }

            // Check exclusions
            if Self::should_exclude(&path, &config.exclude) {
                continue;
            }

            if path.is_dir() {
                Self::add_directory_to_zip(zip, &path, base_path, config, options)?;
            } else {
                Self::add_file_to_zip(zip, &path, base_path, options)?;
            }
        }

        Ok(())
    }

    fn should_exclude(path: &Path, exclude_patterns: &[String]) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in exclude_patterns {
            if pattern.contains('*') {
                // Simple glob matching
                let pattern = pattern.replace("**", ".*").replace('*', "[^/]*");

                if let Ok(re) = regex::Regex::new(&pattern) {
                    if re.is_match(&path_str) {
                        return true;
                    }
                }
            } else if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Upload ZIP to S3 and return the S3 location.
    pub async fn upload_to_s3(
        zip_data: &[u8],
        bucket: &str,
        key: &str,
        region: &str,
    ) -> anyhow::Result<S3Location> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(region.to_string()))
            .load()
            .await;

        let s3_client = aws_sdk_s3::Client::new(&aws_config);

        s3_client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(aws_sdk_s3::primitives::ByteStream::from(zip_data.to_vec()))
            .send()
            .await?;

        tracing::info!("Uploaded ZIP to s3://{}/{}", bucket, key);

        Ok(S3Location {
            bucket: bucket.to_string(),
            key: key.to_string(),
        })
    }
}

/// S3 location for a Lambda deployment package.
#[derive(Debug, Clone)]
pub struct S3Location {
    pub bucket: String,
    pub key: String,
}

// ============================================================================
// Layer Management
// ============================================================================

/// Configuration for a Lambda layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    /// Layer name.
    pub name: String,

    /// Description of the layer.
    pub description: Option<String>,

    /// Source path for layer content (local directory).
    pub source_path: Option<String>,

    /// S3 bucket containing the layer ZIP.
    pub s3_bucket: Option<String>,

    /// S3 key for the layer ZIP.
    pub s3_key: Option<String>,

    /// Compatible runtimes (e.g., ["python3.9", "python3.10"]).
    #[serde(default)]
    pub compatible_runtimes: Vec<String>,

    /// Compatible architectures (e.g., ["x86_64", "arm64"]).
    #[serde(default)]
    pub compatible_architectures: Vec<String>,

    /// License information.
    pub license_info: Option<String>,
}

impl LayerConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

/// Manages Lambda layers.
pub struct LayerManager {
    client: LambdaClient,
}

impl LayerManager {
    pub fn new(client: LambdaClient) -> Self {
        Self { client }
    }

    /// Publish a new layer version.
    pub async fn publish_layer(
        &self,
        config: &LayerConfig,
        region: &str,
    ) -> anyhow::Result<PublishedLayer> {
        let mut builder = self
            .client
            .publish_layer_version()
            .layer_name(&config.name);

        if let Some(desc) = &config.description {
            builder = builder.description(desc);
        }

        // Set layer content
        if let Some(source_path) = &config.source_path {
            let zip_config = ZipPackageConfig {
                source_path: source_path.clone(),
                s3_bucket: None,
                s3_prefix: None,
                exclude: vec![],
                include_hidden: false,
            };
            let zip_data = LambdaPackager::create_zip(&zip_config)?;

            builder = builder.content(
                aws_sdk_lambda::types::LayerVersionContentInput::builder()
                    .zip_file(aws_sdk_lambda::primitives::Blob::new(zip_data))
                    .build(),
            );
        } else if let (Some(bucket), Some(key)) = (&config.s3_bucket, &config.s3_key) {
            builder = builder.content(
                aws_sdk_lambda::types::LayerVersionContentInput::builder()
                    .s3_bucket(bucket)
                    .s3_key(key)
                    .build(),
            );
        } else {
            anyhow::bail!("Layer config must specify either source_path or s3_bucket/s3_key");
        }

        // Set compatible runtimes
        for runtime in &config.compatible_runtimes {
            builder =
                builder.compatible_runtimes(aws_sdk_lambda::types::Runtime::from(runtime.as_str()));
        }

        // Set compatible architectures
        for arch in &config.compatible_architectures {
            builder = builder
                .compatible_architectures(aws_sdk_lambda::types::Architecture::from(arch.as_str()));
        }

        if let Some(license) = &config.license_info {
            builder = builder.license_info(license);
        }

        let response = builder.send().await?;

        let arn = response
            .layer_version_arn
            .ok_or_else(|| anyhow::anyhow!("No layer ARN returned"))?;

        let version = response.version;

        tracing::info!("Published layer {} version {}", config.name, version);

        Ok(PublishedLayer {
            name: config.name.clone(),
            arn,
            version,
        })
    }

    /// List all versions of a layer.
    pub async fn list_layer_versions(&self, layer_name: &str) -> anyhow::Result<Vec<LayerVersion>> {
        let mut versions = Vec::new();
        let mut marker = None;

        loop {
            let mut request = self.client.list_layer_versions().layer_name(layer_name);

            if let Some(m) = marker {
                request = request.marker(m);
            }

            let response = request.send().await?;

            for version in response.layer_versions.unwrap_or_default() {
                versions.push(LayerVersion {
                    version: version.version,
                    arn: version.layer_version_arn.unwrap_or_default(),
                    description: version.description,
                    created_date: version.created_date,
                });
            }

            marker = response.next_marker;

            if marker.is_none() {
                break;
            }
        }

        Ok(versions)
    }

    /// Delete old layer versions, keeping the most recent N versions.
    pub async fn cleanup_old_versions(
        &self,
        layer_name: &str,
        keep_versions: usize,
    ) -> anyhow::Result<usize> {
        let versions = self.list_layer_versions(layer_name).await?;

        if versions.len() <= keep_versions {
            return Ok(0);
        }

        let mut deleted = 0;
        let to_delete = versions.len() - keep_versions;

        // Versions are sorted newest first, so we delete from the end
        for version in versions.iter().rev().take(to_delete) {
            self.client
                .delete_layer_version()
                .layer_name(layer_name)
                .version_number(version.version)
                .send()
                .await?;

            tracing::info!(
                "Deleted layer {} version {}",
                layer_name,
                version.version
            );
            deleted += 1;
        }

        Ok(deleted)
    }

    /// Get the latest layer version ARN.
    pub async fn get_latest_version_arn(&self, layer_name: &str) -> anyhow::Result<String> {
        let versions = self.list_layer_versions(layer_name).await?;

        versions
            .first()
            .map(|v| v.arn.clone())
            .ok_or_else(|| anyhow::anyhow!("No versions found for layer {}", layer_name))
    }
}

/// Published layer information.
#[derive(Debug, Clone)]
pub struct PublishedLayer {
    pub name: String,
    pub arn: String,
    pub version: i64,
}

/// Layer version information.
#[derive(Debug, Clone)]
pub struct LayerVersion {
    pub version: i64,
    pub arn: String,
    pub description: Option<String>,
    pub created_date: Option<String>,
}

// ============================================================================
// Event Source Mappings
// ============================================================================

/// Event source type for Lambda triggers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventSourceType {
    Sqs,
    Sns,
    DynamodbStream,
    KinesisStream,
    Kafka,
    ManagedKafka,
    ActiveMq,
    RabbitMq,
}

impl EventSourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventSourceType::Sqs => "SQS",
            EventSourceType::Sns => "SNS",
            EventSourceType::DynamodbStream => "DynamoDB",
            EventSourceType::KinesisStream => "Kinesis",
            EventSourceType::Kafka => "Kafka",
            EventSourceType::ManagedKafka => "ManagedKafka",
            EventSourceType::ActiveMq => "ActiveMQ",
            EventSourceType::RabbitMq => "RabbitMQ",
        }
    }
}

/// Configuration for an event source mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSourceConfig {
    /// Type of event source.
    pub source_type: EventSourceType,

    /// ARN of the event source (SQS queue, Kinesis stream, etc.).
    pub source_arn: String,

    /// Batch size for event processing.
    #[serde(default = "default_batch_size")]
    pub batch_size: i32,

    /// Maximum batching window in seconds.
    pub maximum_batching_window_secs: Option<i32>,

    /// Starting position for stream-based sources.
    pub starting_position: Option<String>,

    /// Whether the event source mapping is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Filter criteria for event filtering.
    pub filter_patterns: Option<Vec<String>>,

    /// Maximum record age in seconds (for Kinesis/DynamoDB).
    pub maximum_record_age_secs: Option<i32>,

    /// Maximum retry attempts.
    pub maximum_retry_attempts: Option<i32>,

    /// Parallelization factor (for Kinesis/DynamoDB).
    pub parallelization_factor: Option<i32>,

    /// Tumbling window in seconds.
    pub tumbling_window_secs: Option<i32>,

    /// On-failure destination ARN.
    pub on_failure_destination_arn: Option<String>,

    /// Function response types.
    #[serde(default)]
    pub function_response_types: Vec<String>,

    /// Source access configuration (for Kafka, MQ).
    #[serde(default)]
    pub source_access: Vec<SourceAccessEntry>,
}

fn default_batch_size() -> i32 {
    10
}

fn default_true() -> bool {
    true
}

/// Source access configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAccessEntry {
    /// Access type (e.g., "BASIC_AUTH", "VPC_SUBNET", "VPC_SECURITY_GROUP").
    pub access_type: String,
    /// URI for the access configuration.
    pub uri: String,
}

impl EventSourceConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

/// Manages Lambda event source mappings.
pub struct EventSourceManager {
    client: LambdaClient,
    function_name: String,
}

impl EventSourceManager {
    pub fn new(client: LambdaClient, function_name: &str) -> Self {
        Self {
            client,
            function_name: function_name.to_string(),
        }
    }

    /// Create or update an event source mapping.
    pub async fn configure_event_source(
        &self,
        config: &EventSourceConfig,
    ) -> anyhow::Result<EventSourceMapping> {
        // Check if mapping already exists
        let existing = self.find_existing_mapping(&config.source_arn).await?;

        if let Some(existing_mapping) = existing {
            self.update_event_source(&existing_mapping.uuid, config)
                .await
        } else {
            self.create_event_source(config).await
        }
    }

    async fn create_event_source(
        &self,
        config: &EventSourceConfig,
    ) -> anyhow::Result<EventSourceMapping> {
        let mut builder = self
            .client
            .create_event_source_mapping()
            .function_name(&self.function_name)
            .event_source_arn(&config.source_arn)
            .batch_size(config.batch_size)
            .enabled(config.enabled);

        // Set starting position for stream-based sources
        if let Some(pos) = &config.starting_position {
            builder = builder.starting_position(
                aws_sdk_lambda::types::EventSourcePosition::from(pos.as_str()),
            );
        }

        // Set batching window
        if let Some(window) = config.maximum_batching_window_secs {
            builder = builder.maximum_batching_window_in_seconds(window);
        }

        // Set filter criteria
        if let Some(patterns) = &config.filter_patterns {
            let filters: Vec<_> = patterns
                .iter()
                .map(|p| aws_sdk_lambda::types::Filter::builder().pattern(p).build())
                .collect();

            builder = builder.filter_criteria(
                FilterCriteria::builder()
                    .set_filters(Some(filters))
                    .build(),
            );
        }

        // Set stream-specific options
        if let Some(age) = config.maximum_record_age_secs {
            builder = builder.maximum_record_age_in_seconds(age);
        }

        if let Some(retries) = config.maximum_retry_attempts {
            builder = builder.maximum_retry_attempts(retries);
        }

        if let Some(factor) = config.parallelization_factor {
            builder = builder.parallelization_factor(factor);
        }

        if let Some(window) = config.tumbling_window_secs {
            builder = builder.tumbling_window_in_seconds(window);
        }

        // Set on-failure destination
        if let Some(dest_arn) = &config.on_failure_destination_arn {
            builder = builder.destination_config(
                aws_sdk_lambda::types::DestinationConfig::builder()
                    .on_failure(
                        aws_sdk_lambda::types::OnFailure::builder()
                            .destination(dest_arn)
                            .build(),
                    )
                    .build(),
            );
        }

        // Set function response types
        for response_type in &config.function_response_types {
            builder = builder.function_response_types(
                aws_sdk_lambda::types::FunctionResponseType::from(response_type.as_str()),
            );
        }

        // Set source access configuration
        for access in &config.source_access {
            builder = builder.source_access_configurations(
                SourceAccessConfiguration::builder()
                    .r#type(SourceAccessType::from(access.access_type.as_str()))
                    .uri(&access.uri)
                    .build(),
            );
        }

        let response = builder.send().await?;

        let uuid = response
            .uuid
            .ok_or_else(|| anyhow::anyhow!("No UUID returned for event source mapping"))?;

        tracing::info!(
            "Created event source mapping {} for {} -> {}",
            uuid,
            config.source_arn,
            self.function_name
        );

        Ok(EventSourceMapping {
            uuid,
            source_arn: config.source_arn.clone(),
            function_arn: response.function_arn.unwrap_or_default(),
            state: response.state.map(|s| s.to_string()),
            batch_size: response.batch_size,
        })
    }

    async fn update_event_source(
        &self,
        uuid: &str,
        config: &EventSourceConfig,
    ) -> anyhow::Result<EventSourceMapping> {
        let mut builder = self
            .client
            .update_event_source_mapping()
            .uuid(uuid)
            .function_name(&self.function_name)
            .batch_size(config.batch_size)
            .enabled(config.enabled);

        if let Some(window) = config.maximum_batching_window_secs {
            builder = builder.maximum_batching_window_in_seconds(window);
        }

        if let Some(patterns) = &config.filter_patterns {
            let filters: Vec<_> = patterns
                .iter()
                .map(|p| aws_sdk_lambda::types::Filter::builder().pattern(p).build())
                .collect();

            builder = builder.filter_criteria(
                FilterCriteria::builder()
                    .set_filters(Some(filters))
                    .build(),
            );
        }

        if let Some(age) = config.maximum_record_age_secs {
            builder = builder.maximum_record_age_in_seconds(age);
        }

        if let Some(retries) = config.maximum_retry_attempts {
            builder = builder.maximum_retry_attempts(retries);
        }

        if let Some(factor) = config.parallelization_factor {
            builder = builder.parallelization_factor(factor);
        }

        let response = builder.send().await?;

        tracing::info!(
            "Updated event source mapping {} for {}",
            uuid,
            self.function_name
        );

        Ok(EventSourceMapping {
            uuid: uuid.to_string(),
            source_arn: config.source_arn.clone(),
            function_arn: response.function_arn.unwrap_or_default(),
            state: response.state.map(|s| s.to_string()),
            batch_size: response.batch_size,
        })
    }

    /// Find an existing event source mapping by source ARN.
    async fn find_existing_mapping(
        &self,
        source_arn: &str,
    ) -> anyhow::Result<Option<EventSourceMapping>> {
        let mappings = self.list_event_sources().await?;

        Ok(mappings.into_iter().find(|m| m.source_arn == source_arn))
    }

    /// List all event source mappings for the function.
    pub async fn list_event_sources(&self) -> anyhow::Result<Vec<EventSourceMapping>> {
        let mut mappings = Vec::new();
        let mut marker = None;

        loop {
            let mut request = self
                .client
                .list_event_source_mappings()
                .function_name(&self.function_name);

            if let Some(m) = marker {
                request = request.marker(m);
            }

            let response = request.send().await?;

            for mapping in response.event_source_mappings.unwrap_or_default() {
                mappings.push(EventSourceMapping {
                    uuid: mapping.uuid.unwrap_or_default(),
                    source_arn: mapping.event_source_arn.unwrap_or_default(),
                    function_arn: mapping.function_arn.unwrap_or_default(),
                    state: mapping.state.map(|s| s.to_string()),
                    batch_size: mapping.batch_size,
                });
            }

            marker = response.next_marker;

            if marker.is_none() {
                break;
            }
        }

        Ok(mappings)
    }

    /// Delete an event source mapping.
    pub async fn delete_event_source(&self, uuid: &str) -> anyhow::Result<()> {
        self.client
            .delete_event_source_mapping()
            .uuid(uuid)
            .send()
            .await?;

        tracing::info!(
            "Deleted event source mapping {} for {}",
            uuid,
            self.function_name
        );

        Ok(())
    }

    /// Delete all event source mappings for the function.
    pub async fn delete_all_event_sources(&self) -> anyhow::Result<usize> {
        let mappings = self.list_event_sources().await?;
        let count = mappings.len();

        for mapping in mappings {
            self.delete_event_source(&mapping.uuid).await?;
        }

        Ok(count)
    }
}

/// Event source mapping information.
#[derive(Debug, Clone)]
pub struct EventSourceMapping {
    pub uuid: String,
    pub source_arn: String,
    pub function_arn: String,
    pub state: Option<String>,
    pub batch_size: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_package_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
source_path: ./src
s3_bucket: my-bucket
s3_prefix: lambda-code
exclude:
  - "*.pyc"
  - "__pycache__"
include_hidden: false
"#,
        )
        .unwrap();

        let config = ZipPackageConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.source_path, "./src");
        assert_eq!(config.s3_bucket, Some("my-bucket".to_string()));
        assert_eq!(config.exclude.len(), 2);
        assert!(!config.include_hidden);
    }

    #[test]
    fn test_zip_package_s3_key() {
        let config = ZipPackageConfig {
            source_path: "./src".to_string(),
            s3_bucket: Some("bucket".to_string()),
            s3_prefix: Some("code".to_string()),
            exclude: vec![],
            include_hidden: false,
        };

        let key = config.s3_key("my-function");
        assert!(key.starts_with("code/my-function-"));
        assert!(key.ends_with(".zip"));
    }

    #[test]
    fn test_layer_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
name: my-layer
description: Common dependencies
source_path: ./layers/common
compatible_runtimes:
  - python3.9
  - python3.10
compatible_architectures:
  - x86_64
"#,
        )
        .unwrap();

        let config = LayerConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.name, "my-layer");
        assert_eq!(config.compatible_runtimes.len(), 2);
        assert_eq!(config.compatible_architectures.len(), 1);
    }

    #[test]
    fn test_event_source_type() {
        assert_eq!(EventSourceType::Sqs.as_str(), "SQS");
        assert_eq!(EventSourceType::DynamodbStream.as_str(), "DynamoDB");
        assert_eq!(EventSourceType::KinesisStream.as_str(), "Kinesis");
    }

    #[test]
    fn test_event_source_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
source_type: sqs
source_arn: arn:aws:sqs:us-east-1:123456789:my-queue
batch_size: 10
maximum_batching_window_secs: 5
enabled: true
filter_patterns:
  - '{"body": {"type": ["order"]}}'
"#,
        )
        .unwrap();

        let config = EventSourceConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.source_type, EventSourceType::Sqs);
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.maximum_batching_window_secs, Some(5));
        assert!(config.enabled);
        assert_eq!(config.filter_patterns.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_kinesis_event_source_config() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
source_type: kinesis_stream
source_arn: arn:aws:kinesis:us-east-1:123456789:stream/my-stream
batch_size: 100
starting_position: LATEST
parallelization_factor: 2
maximum_record_age_secs: 3600
maximum_retry_attempts: 3
"#,
        )
        .unwrap();

        let config = EventSourceConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.source_type, EventSourceType::KinesisStream);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.starting_position, Some("LATEST".to_string()));
        assert_eq!(config.parallelization_factor, Some(2));
        assert_eq!(config.maximum_record_age_secs, Some(3600));
    }
}
