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

const API_BASE: &str = "https://api.replicate.com/v1";

pub struct ReplicateProvider {
    client: Client,
    api_key: String,
}

impl ReplicateProvider {
    pub fn new(api_key: String) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Token {}", api_key))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { client, api_key })
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("REPLICATE_API_TOKEN")
            .context("REPLICATE_API_TOKEN environment variable not set")?;
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

            let response: ReplicatePrediction = self
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
                    // Exponential backoff up to 5 seconds
                    poll_interval = std::cmp::min(poll_interval * 2, Duration::from_secs(5));
                }
            }
        }
    }
}

#[async_trait]
impl ModelProvider for ReplicateProvider {
    fn name(&self) -> &'static str {
        "replicate"
    }

    async fn run(
        &self,
        model: &str,
        mut input: Value,
        files: &HashMap<String, FileUpload>,
        wait: bool,
        timeout: u64,
    ) -> Result<PredictionResult> {
        // Process file uploads - convert to data URIs
        if let Value::Object(ref mut map) = input {
            for (param_name, file_upload) in files {
                let data_uri = file_to_data_uri(&file_upload.path).await?;
                map.insert(param_name.clone(), Value::String(data_uri));
            }
        }

        // Parse model identifier (owner/name or owner/name:version)
        let (version, model_ref) = if model.contains(':') {
            let parts: Vec<&str> = model.splitn(2, ':').collect();
            (Some(parts[1].to_string()), parts[0])
        } else {
            // Need to fetch the latest version
            let schema = self.get_schema(model).await?;
            let version = schema
                .input_schema
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            (version, model)
        };

        let request_body = if let Some(ref ver) = version {
            serde_json::json!({
                "version": ver,
                "input": input
            })
        } else {
            // Use model identifier for official models
            serde_json::json!({
                "model": model_ref,
                "input": input
            })
        };

        let response: ReplicatePrediction = self
            .client
            .post(format!("{}/predictions", API_BASE))
            .json(&request_body)
            .send()
            .await?
            .error_for_status()
            .context("Failed to create prediction")?
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
        // Model format: owner/name or owner/name:version
        let model_path = model.split(':').next().unwrap_or(model);

        let response: ReplicateModel = self
            .client
            .get(format!("{}/models/{}", API_BASE, model_path))
            .send()
            .await?
            .error_for_status()
            .context("Failed to fetch model schema")?
            .json()
            .await?;

        let latest_version = response.latest_version.unwrap_or_default();

        Ok(ModelSchema {
            name: response.name,
            description: response.description,
            input_schema: latest_version
                .openapi_schema
                .get("components")
                .and_then(|c| c.get("schemas"))
                .and_then(|s| s.get("Input"))
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::json!({
                        "version": latest_version.id
                    })
                }),
            output_schema: latest_version
                .openapi_schema
                .get("components")
                .and_then(|c| c.get("schemas"))
                .and_then(|s| s.get("Output"))
                .cloned(),
        })
    }

    async fn list_models(&self, query: Option<&str>, limit: usize) -> Result<Vec<ModelInfo>> {
        let url = if let Some(q) = query {
            format!("{}/models?query={}", API_BASE, urlencoding::encode(q))
        } else {
            format!("{}/models", API_BASE)
        };

        let response: ReplicateModelList = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(response
            .results
            .into_iter()
            .take(limit)
            .map(|m| ModelInfo {
                id: format!("{}/{}", m.owner, m.name),
                name: m.name,
                description: m.description,
                owner: Some(m.owner),
                url: m.url,
            })
            .collect())
    }

    async fn upload_file(&self, path: &Path) -> Result<String> {
        // Replicate accepts data URIs directly, so we convert to that
        file_to_data_uri(path).await
    }
}

// Replicate API types

#[derive(Debug, Deserialize)]
struct ReplicatePrediction {
    id: String,
    status: String,
    output: Option<Value>,
    error: Option<String>,
    metrics: Option<ReplicateMetrics>,
    urls: Option<ReplicateUrls>,
}

impl ReplicatePrediction {
    fn into_prediction_result(self) -> PredictionResult {
        PredictionResult {
            id: self.id,
            status: match self.status.as_str() {
                "starting" => PredictionStatus::Starting,
                "processing" => PredictionStatus::Processing,
                "succeeded" => PredictionStatus::Succeeded,
                "failed" => PredictionStatus::Failed,
                "canceled" => PredictionStatus::Canceled,
                _ => PredictionStatus::Processing,
            },
            output: self.output,
            error: self.error,
            metrics: self.metrics.map(|m| PredictionMetrics {
                predict_time: m.predict_time,
                total_time: m.total_time,
            }),
            urls: self.urls.map(|u| PredictionUrls {
                get: Some(u.get),
                cancel: Some(u.cancel),
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReplicateMetrics {
    predict_time: Option<f64>,
    total_time: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ReplicateUrls {
    get: String,
    cancel: String,
}

#[derive(Debug, Deserialize)]
struct ReplicateModel {
    name: String,
    description: Option<String>,
    owner: String,
    latest_version: Option<ReplicateVersion>,
}

#[derive(Debug, Default, Deserialize)]
struct ReplicateVersion {
    id: String,
    openapi_schema: Value,
}

#[derive(Debug, Deserialize)]
struct ReplicateModelList {
    results: Vec<ReplicateModelListItem>,
}

#[derive(Debug, Deserialize)]
struct ReplicateModelListItem {
    name: String,
    owner: String,
    description: Option<String>,
    url: Option<String>,
}
