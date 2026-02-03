use super::{
    FileUpload, ModelInfo, ModelProvider, ModelSchema, PredictionMetrics,
    PredictionResult, PredictionStatus, PredictionUrls,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const QUEUE_API_BASE: &str = "https://queue.fal.run";
const UPLOAD_API_BASE: &str = "https://rest.fal.run/storage/upload/url";

pub struct FalProvider {
    client: Client,
    api_key: String,
}

impl FalProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Key {}", api_key))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(300))
            .build()?;

        Ok(Self { client, api_key })
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("FAL_KEY")
            .or_else(|_| std::env::var("FAL_API_KEY"))
            .context("FAL_KEY or FAL_API_KEY environment variable not set")?;
        Self::new(api_key)
    }

    async fn poll_request(&self, request_id: &str, model: &str, timeout: u64) -> Result<PredictionResult> {
        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(timeout);
        let mut poll_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout_duration {
                anyhow::bail!("Request timed out after {} seconds", timeout);
            }

            let status_url = format!("{}/{}/requests/{}/status", QUEUE_API_BASE, model, request_id);
            let response: FalQueueStatus = self
                .client
                .get(&status_url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            match response.status.as_str() {
                "COMPLETED" => {
                    // Fetch the result
                    let result_url = format!("{}/{}/requests/{}", QUEUE_API_BASE, model, request_id);
                    let result: FalResult = self
                        .client
                        .get(&result_url)
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;

                    return Ok(PredictionResult {
                        id: request_id.to_string(),
                        status: PredictionStatus::Succeeded,
                        output: Some(result.data),
                        error: None,
                        metrics: result.metrics.map(|m| PredictionMetrics {
                            predict_time: m.inference_time,
                            total_time: m.total_time,
                        }),
                        urls: None,
                    });
                }
                "FAILED" => {
                    return Ok(PredictionResult {
                        id: request_id.to_string(),
                        status: PredictionStatus::Failed,
                        output: None,
                        error: response.error,
                        metrics: None,
                        urls: None,
                    });
                }
                "IN_QUEUE" | "IN_PROGRESS" => {
                    tokio::time::sleep(poll_interval).await;
                    poll_interval = std::cmp::min(poll_interval * 2, Duration::from_secs(5));
                }
                _ => {
                    tokio::time::sleep(poll_interval).await;
                    poll_interval = std::cmp::min(poll_interval * 2, Duration::from_secs(5));
                }
            }
        }
    }

    async fn upload_to_fal_storage(&self, path: &Path) -> Result<String> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        let file_upload = FileUpload::new(path);
        let content_type = file_upload.detect_mime_type();

        // First, request an upload URL
        let upload_request = serde_json::json!({
            "file_name": file_name,
            "content_type": content_type
        });

        let upload_info: FalUploadInfo = self
            .client
            .post(UPLOAD_API_BASE)
            .json(&upload_request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        // Upload the file to the presigned URL
        let file_content = tokio::fs::read(path).await?;

        reqwest::Client::new()
            .put(&upload_info.upload_url)
            .header(header::CONTENT_TYPE, &content_type)
            .body(file_content)
            .send()
            .await?
            .error_for_status()?;

        Ok(upload_info.file_url)
    }
}

#[async_trait]
impl ModelProvider for FalProvider {
    fn name(&self) -> &'static str {
        "fal"
    }

    async fn run(
        &self,
        model: &str,
        mut input: Value,
        files: &HashMap<String, FileUpload>,
        wait: bool,
        timeout: u64,
    ) -> Result<PredictionResult> {
        // Process file uploads - upload to Fal storage and use URLs
        if let Value::Object(ref mut map) = input {
            for (param_name, file_upload) in files {
                let file_url = self.upload_to_fal_storage(&file_upload.path).await?;
                map.insert(param_name.clone(), Value::String(file_url));
            }
        }

        // Use queue API for async execution
        let url = format!("{}/{}", QUEUE_API_BASE, model);
        let response: FalQueueResponse = self
            .client
            .post(&url)
            .json(&input)
            .send()
            .await?
            .error_for_status()
            .context("Failed to submit request to Fal")?
            .json()
            .await?;

        let result = PredictionResult {
            id: response.request_id.clone(),
            status: PredictionStatus::Starting,
            output: None,
            error: None,
            metrics: None,
            urls: Some(PredictionUrls {
                get: Some(format!("{}/{}/requests/{}", QUEUE_API_BASE, model, response.request_id)),
                cancel: Some(format!("{}/{}/requests/{}/cancel", QUEUE_API_BASE, model, response.request_id)),
            }),
        };

        if wait {
            self.poll_request(&response.request_id, model, timeout).await
        } else {
            Ok(result)
        }
    }

    async fn get_schema(&self, model: &str) -> Result<ModelSchema> {
        // Fal doesn't have a dedicated schema endpoint, so we return basic info
        // The schema is typically available in their documentation
        Ok(ModelSchema {
            name: model.to_string(),
            description: Some(format!(
                "Fal model: {}. See https://fal.ai/models/{} for documentation.",
                model, model
            )),
            input_schema: serde_json::json!({
                "type": "object",
                "description": "Input schema varies by model. Visit https://fal.ai/models for details."
            }),
            output_schema: None,
        })
    }

    async fn list_models(&self, _query: Option<&str>, _limit: usize) -> Result<Vec<ModelInfo>> {
        // Fal doesn't have a public model listing API
        // Return some popular models as examples
        Ok(vec![
            ModelInfo {
                id: "fal-ai/flux/dev".to_string(),
                name: "FLUX.1 [dev]".to_string(),
                description: Some("High-quality image generation model".to_string()),
                owner: Some("fal-ai".to_string()),
                url: Some("https://fal.ai/models/fal-ai/flux/dev".to_string()),
            },
            ModelInfo {
                id: "fal-ai/flux/schnell".to_string(),
                name: "FLUX.1 [schnell]".to_string(),
                description: Some("Fast image generation model".to_string()),
                owner: Some("fal-ai".to_string()),
                url: Some("https://fal.ai/models/fal-ai/flux/schnell".to_string()),
            },
            ModelInfo {
                id: "fal-ai/stable-diffusion-v3-medium".to_string(),
                name: "Stable Diffusion 3 Medium".to_string(),
                description: Some("Stable Diffusion 3 medium model".to_string()),
                owner: Some("fal-ai".to_string()),
                url: Some("https://fal.ai/models/fal-ai/stable-diffusion-v3-medium".to_string()),
            },
            ModelInfo {
                id: "fal-ai/whisper".to_string(),
                name: "Whisper".to_string(),
                description: Some("Speech-to-text model".to_string()),
                owner: Some("fal-ai".to_string()),
                url: Some("https://fal.ai/models/fal-ai/whisper".to_string()),
            },
            ModelInfo {
                id: "fal-ai/sadtalker".to_string(),
                name: "SadTalker".to_string(),
                description: Some("Audio-driven talking head synthesis".to_string()),
                owner: Some("fal-ai".to_string()),
                url: Some("https://fal.ai/models/fal-ai/sadtalker".to_string()),
            },
        ])
    }

    async fn upload_file(&self, path: &Path) -> Result<String> {
        self.upload_to_fal_storage(path).await
    }
}

// Fal API types

#[derive(Debug, Deserialize)]
struct FalQueueResponse {
    request_id: String,
}

#[derive(Debug, Deserialize)]
struct FalQueueStatus {
    status: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FalResult {
    #[serde(flatten)]
    data: Value,
    #[serde(default)]
    metrics: Option<FalMetrics>,
}

#[derive(Debug, Deserialize)]
struct FalMetrics {
    inference_time: Option<f64>,
    total_time: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FalUploadInfo {
    upload_url: String,
    file_url: String,
}
