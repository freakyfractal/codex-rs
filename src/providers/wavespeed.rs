use super::{
    file_to_data_uri, FileUpload, ModelInfo, ModelProvider, ModelSchema, PredictionMetrics,
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

const API_BASE: &str = "https://api.wavespeed.ai/v1";

pub struct WaveSpeedProvider {
    client: Client,
    api_key: String,
}

impl WaveSpeedProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", api_key))?,
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
        let api_key = std::env::var("WAVESPEED_API_KEY")
            .context("WAVESPEED_API_KEY environment variable not set")?;
        Self::new(api_key)
    }

    async fn poll_prediction(&self, id: &str, timeout: u64) -> Result<PredictionResult> {
        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(timeout);
        let mut poll_interval = Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout_duration {
                anyhow::bail!("Prediction timed out after {} seconds", timeout);
            }

            let response: WaveSpeedPrediction = self
                .client
                .get(format!("{}/predictions/{}", API_BASE, id))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            let result = response.into_prediction_result();

            match result.status {
                PredictionStatus::Succeeded | PredictionStatus::Failed | PredictionStatus::Canceled => {
                    return Ok(result);
                }
                _ => {
                    tokio::time::sleep(poll_interval).await;
                    poll_interval = std::cmp::min(poll_interval * 2, Duration::from_secs(5));
                }
            }
        }
    }
}

#[async_trait]
impl ModelProvider for WaveSpeedProvider {
    fn name(&self) -> &'static str {
        "wavespeed"
    }

    async fn run(
        &self,
        model: &str,
        mut input: Value,
        files: &HashMap<String, FileUpload>,
        wait: bool,
        timeout: u64,
    ) -> Result<PredictionResult> {
        // Process file uploads - convert to data URIs or upload to their storage
        if let Value::Object(ref mut map) = input {
            for (param_name, file_upload) in files {
                // WaveSpeed accepts base64 data URIs
                let data_uri = file_to_data_uri(&file_upload.path).await?;
                map.insert(param_name.clone(), Value::String(data_uri));
            }
        }

        let request_body = serde_json::json!({
            "model": model,
            "input": input
        });

        let response: WaveSpeedPrediction = self
            .client
            .post(format!("{}/predictions", API_BASE))
            .json(&request_body)
            .send()
            .await?
            .error_for_status()
            .context("Failed to create prediction on WaveSpeed")?
            .json()
            .await?;

        let result = response.into_prediction_result();

        if wait {
            self.poll_prediction(&result.id, timeout).await
        } else {
            Ok(result)
        }
    }

    async fn get_schema(&self, model: &str) -> Result<ModelSchema> {
        // Try to fetch model information
        let response: Result<WaveSpeedModel, _> = self
            .client
            .get(format!("{}/models/{}", API_BASE, model))
            .send()
            .await?
            .error_for_status()
            .context("Failed to fetch model info")?
            .json()
            .await;

        match response {
            Ok(model_info) => Ok(ModelSchema {
                name: model_info.name.unwrap_or_else(|| model.to_string()),
                description: model_info.description,
                input_schema: model_info.input_schema.unwrap_or_else(|| {
                    serde_json::json!({
                        "type": "object",
                        "description": "See WaveSpeed documentation for input schema"
                    })
                }),
                output_schema: model_info.output_schema,
            }),
            Err(_) => Ok(ModelSchema {
                name: model.to_string(),
                description: Some(format!(
                    "WaveSpeed model: {}. See https://wavespeed.ai for documentation.",
                    model
                )),
                input_schema: serde_json::json!({
                    "type": "object",
                    "description": "Input schema varies by model. Visit WaveSpeed documentation for details."
                }),
                output_schema: None,
            }),
        }
    }

    async fn list_models(&self, query: Option<&str>, limit: usize) -> Result<Vec<ModelInfo>> {
        let url = if let Some(q) = query {
            format!("{}/models?search={}", API_BASE, urlencoding::encode(q))
        } else {
            format!("{}/models", API_BASE)
        };

        let response: Result<WaveSpeedModelList, _> = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await;

        match response {
            Ok(list) => Ok(list
                .models
                .into_iter()
                .take(limit)
                .map(|m| ModelInfo {
                    id: m.id,
                    name: m.name.unwrap_or_default(),
                    description: m.description,
                    owner: m.owner,
                    url: m.url,
                })
                .collect()),
            Err(_) => {
                // Return some example models if API doesn't support listing
                Ok(vec![
                    ModelInfo {
                        id: "wavespeed/flux-dev".to_string(),
                        name: "FLUX Dev".to_string(),
                        description: Some("FLUX image generation model".to_string()),
                        owner: Some("wavespeed".to_string()),
                        url: Some("https://wavespeed.ai/models/flux-dev".to_string()),
                    },
                    ModelInfo {
                        id: "wavespeed/sd-xl".to_string(),
                        name: "Stable Diffusion XL".to_string(),
                        description: Some("Stable Diffusion XL model".to_string()),
                        owner: Some("wavespeed".to_string()),
                        url: Some("https://wavespeed.ai/models/sd-xl".to_string()),
                    },
                ])
            }
        }
    }

    async fn upload_file(&self, path: &Path) -> Result<String> {
        // WaveSpeed accepts data URIs
        file_to_data_uri(path).await
    }
}

// WaveSpeed API types

#[derive(Debug, Deserialize)]
struct WaveSpeedPrediction {
    id: String,
    status: String,
    output: Option<Value>,
    error: Option<String>,
    #[serde(default)]
    metrics: Option<WaveSpeedMetrics>,
}

impl WaveSpeedPrediction {
    fn into_prediction_result(self) -> PredictionResult {
        PredictionResult {
            id: self.id.clone(),
            status: match self.status.to_lowercase().as_str() {
                "pending" | "starting" => PredictionStatus::Starting,
                "processing" | "running" => PredictionStatus::Processing,
                "succeeded" | "completed" | "success" => PredictionStatus::Succeeded,
                "failed" | "error" => PredictionStatus::Failed,
                "canceled" | "cancelled" => PredictionStatus::Canceled,
                _ => PredictionStatus::Processing,
            },
            output: self.output,
            error: self.error,
            metrics: self.metrics.map(|m| PredictionMetrics {
                predict_time: m.inference_time,
                total_time: m.total_time,
            }),
            urls: Some(PredictionUrls {
                get: Some(format!("{}/predictions/{}", API_BASE, self.id)),
                cancel: None,
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct WaveSpeedMetrics {
    inference_time: Option<f64>,
    total_time: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WaveSpeedModel {
    name: Option<String>,
    description: Option<String>,
    input_schema: Option<Value>,
    output_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct WaveSpeedModelList {
    models: Vec<WaveSpeedModelListItem>,
}

#[derive(Debug, Deserialize)]
struct WaveSpeedModelListItem {
    id: String,
    name: Option<String>,
    description: Option<String>,
    owner: Option<String>,
    url: Option<String>,
}
