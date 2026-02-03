pub mod fal;
pub mod replicate;
pub mod wavespeed;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Trait defining the interface for AI model providers
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Run a model with the given input
    async fn run(
        &self,
        model: &str,
        input: Value,
        files: &HashMap<String, FileUpload>,
        wait: bool,
        timeout: u64,
    ) -> Result<PredictionResult>;

    /// Get the schema/API information for a model
    async fn get_schema(&self, model: &str) -> Result<ModelSchema>;

    /// List available models
    async fn list_models(&self, query: Option<&str>, limit: usize) -> Result<Vec<ModelInfo>>;

    /// Upload a file and return a URL or identifier
    async fn upload_file(&self, path: &Path) -> Result<String>;
}

/// Represents a file to be uploaded
#[derive(Debug, Clone)]
pub struct FileUpload {
    pub path: std::path::PathBuf,
    pub mime_type: Option<String>,
}

impl FileUpload {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            mime_type: None,
        }
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Detect MIME type from file extension
    pub fn detect_mime_type(&self) -> String {
        self.mime_type.clone().unwrap_or_else(|| {
            let ext = self
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            match ext.to_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "mp3" => "audio/mpeg",
                "wav" => "audio/wav",
                "mp4" => "video/mp4",
                "webm" => "video/webm",
                "pdf" => "application/pdf",
                "txt" => "text/plain",
                "json" => "application/json",
                _ => "application/octet-stream",
            }
            .to_string()
        })
    }
}

/// Result of a prediction/inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub id: String,
    pub status: PredictionStatus,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub metrics: Option<PredictionMetrics>,
    pub urls: Option<PredictionUrls>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PredictionStatus {
    Starting,
    Processing,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionMetrics {
    pub predict_time: Option<f64>,
    pub total_time: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionUrls {
    pub get: Option<String>,
    pub cancel: Option<String>,
}

/// Schema information for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSchema {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
}

/// Basic model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub url: Option<String>,
}

/// Helper to read file as base64 data URI
pub async fn file_to_data_uri(path: &Path) -> Result<String> {
    let content = tokio::fs::read(path).await?;
    let mime_type = FileUpload::new(path).detect_mime_type();
    let base64_content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &content);
    Ok(format!("data:{};base64,{}", mime_type, base64_content))
}

/// Parse file arguments in the format "param_name=path/to/file"
pub fn parse_file_args(args: &[String]) -> Result<HashMap<String, FileUpload>> {
    let mut files = HashMap::new();
    for arg in args {
        let parts: Vec<&str> = arg.splitn(2, '=').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid file argument format: '{}'. Expected 'param_name=path'", arg);
        }
        let param_name = parts[0].to_string();
        let path = std::path::PathBuf::from(parts[1]);
        if !path.exists() {
            anyhow::bail!("File not found: {}", path.display());
        }
        files.insert(param_name, FileUpload::new(path));
    }
    Ok(files)
}
