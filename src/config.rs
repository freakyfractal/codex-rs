use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::Provider;

const CONFIG_FILE_NAME: &str = "codex-rs.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub api_keys: HashMap<String, String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .context("Failed to read config file")?;
            let config: Config = serde_json::from_str(&content)
                .context("Failed to parse config file")?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(&path, content)
            .context("Failed to write config file")?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?;
        Ok(config_dir.join("codex-rs").join(CONFIG_FILE_NAME))
    }

    pub fn get_api_key(&self, provider: Provider) -> Option<&String> {
        self.api_keys.get(&provider.to_string())
    }

    pub fn set_api_key(&mut self, provider: Provider, key: String) {
        self.api_keys.insert(provider.to_string(), key);
    }

    pub fn clear_api_key(&mut self, provider: Provider) {
        self.api_keys.remove(&provider.to_string());
    }

    pub fn clear_all(&mut self) {
        self.api_keys.clear();
    }
}

/// Get API key for a provider, checking environment variables first, then config
pub fn get_api_key_for_provider(provider: Provider, config: &Config) -> Result<String> {
    // Check environment variables first
    let env_key = match provider {
        Provider::Replicate => std::env::var("REPLICATE_API_TOKEN").ok(),
        Provider::Fal => std::env::var("FAL_KEY")
            .or_else(|_| std::env::var("FAL_API_KEY"))
            .ok(),
        Provider::WaveSpeed => std::env::var("WAVESPEED_API_KEY").ok(),
    };

    if let Some(key) = env_key {
        return Ok(key);
    }

    // Fall back to config file
    config
        .get_api_key(provider)
        .cloned()
        .with_context(|| {
            let env_var = match provider {
                Provider::Replicate => "REPLICATE_API_TOKEN",
                Provider::Fal => "FAL_KEY",
                Provider::WaveSpeed => "WAVESPEED_API_KEY",
            };
            format!(
                "No API key found for {}. Set {} or run `codex-rs config set {} <key>`",
                provider, env_var, provider
            )
        })
}
