use anyhow::{Context, Result};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Editor, Input, Select};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::{OutputFormat, Provider};
use crate::config::{get_api_key_for_provider, Config};
use crate::output::print_result;
use crate::providers::{
    fal::FalProvider, replicate::ReplicateProvider, wavespeed::WaveSpeedProvider, FileUpload,
    ModelProvider,
};

pub async fn run_interactive(provider: Option<Provider>) -> Result<()> {
    let theme = ColorfulTheme::default();

    println!("{}", style("Welcome to codex-rs interactive mode!").cyan().bold());
    println!();

    // Select provider if not specified
    let provider = match provider {
        Some(p) => p,
        None => {
            let providers = vec!["Replicate", "Fal", "WaveSpeed"];
            let selection = Select::with_theme(&theme)
                .with_prompt("Select a provider")
                .items(&providers)
                .default(0)
                .interact()?;

            match selection {
                0 => Provider::Replicate,
                1 => Provider::Fal,
                2 => Provider::WaveSpeed,
                _ => unreachable!(),
            }
        }
    };

    println!("{} {}", style("Using provider:").dim(), style(provider.to_string()).green());

    // Load config and get API key
    let config = Config::load().unwrap_or_default();
    let api_key = match get_api_key_for_provider(provider, &config) {
        Ok(key) => key,
        Err(_) => {
            println!();
            println!("{}", style("No API key found for this provider.").yellow());
            let key: String = Input::with_theme(&theme)
                .with_prompt("Enter your API key")
                .interact_text()?;

            if Confirm::with_theme(&theme)
                .with_prompt("Save this key to config?")
                .default(true)
                .interact()?
            {
                let mut config = config;
                config.set_api_key(provider, key.clone());
                config.save()?;
                println!("{}", style("Key saved!").green());
            }
            key
        }
    };

    // Create the provider
    let model_provider: Box<dyn ModelProvider> = match provider {
        Provider::Replicate => Box::new(ReplicateProvider::new(api_key)?),
        Provider::Fal => Box::new(FalProvider::new(api_key)?),
        Provider::WaveSpeed => Box::new(WaveSpeedProvider::new(api_key)?),
    };

    // List available models or enter model ID
    println!();
    let use_model_list = Confirm::with_theme(&theme)
        .with_prompt("Would you like to see available models?")
        .default(true)
        .interact()?;

    let model: String = if use_model_list {
        match model_provider.list_models(None, 20).await {
            Ok(models) if !models.is_empty() => {
                let model_names: Vec<String> = models
                    .iter()
                    .map(|m| {
                        let desc = m.description.as_deref().unwrap_or("");
                        let short_desc = if desc.len() > 50 {
                            format!("{}...", &desc[..47])
                        } else {
                            desc.to_string()
                        };
                        format!("{} - {}", m.id, short_desc)
                    })
                    .collect();

                let selection = Select::with_theme(&theme)
                    .with_prompt("Select a model")
                    .items(&model_names)
                    .default(0)
                    .interact()?;

                models[selection].id.clone()
            }
            _ => {
                Input::with_theme(&theme)
                    .with_prompt("Enter model ID")
                    .interact_text()?
            }
        }
    } else {
        Input::with_theme(&theme)
            .with_prompt("Enter model ID")
            .interact_text()?
    };

    println!("{} {}", style("Selected model:").dim(), style(&model).green());

    // Try to fetch schema for guidance
    println!();
    println!("{}", style("Fetching model schema...").dim());

    match model_provider.get_schema(&model).await {
        Ok(schema) => {
            if let Some(desc) = &schema.description {
                println!("{} {}", style("Description:").dim(), desc);
            }
            println!("{} {}", style("Input schema:").dim(),
                serde_json::to_string_pretty(&schema.input_schema).unwrap_or_default());
        }
        Err(e) => {
            println!("{} {}", style("Could not fetch schema:").yellow(), e);
        }
    }

    // Get input
    println!();
    let input_method = Select::with_theme(&theme)
        .with_prompt("How would you like to provide input?")
        .items(&["Enter JSON manually", "Open editor", "Use default/example"])
        .default(0)
        .interact()?;

    let input: Value = match input_method {
        0 => {
            let input_str: String = Input::with_theme(&theme)
                .with_prompt("Enter input JSON")
                .default("{}".to_string())
                .interact_text()?;
            serde_json::from_str(&input_str).context("Invalid JSON")?
        }
        1 => {
            let edited = Editor::new()
                .extension(".json")
                .edit("{}")?
                .unwrap_or_else(|| "{}".to_string());
            serde_json::from_str(&edited).context("Invalid JSON")?
        }
        _ => serde_json::json!({}),
    };

    // Ask about file uploads
    let mut files: HashMap<String, FileUpload> = HashMap::new();

    if Confirm::with_theme(&theme)
        .with_prompt("Do you want to upload any files?")
        .default(false)
        .interact()?
    {
        loop {
            let param_name: String = Input::with_theme(&theme)
                .with_prompt("Parameter name for file (e.g., 'image', 'audio')")
                .interact_text()?;

            let file_path: String = Input::with_theme(&theme)
                .with_prompt("File path")
                .interact_text()?;

            let path = PathBuf::from(&file_path);
            if path.exists() {
                files.insert(param_name, FileUpload::new(path));
                println!("{}", style("File added!").green());
            } else {
                println!("{}", style("File not found, skipping.").yellow());
            }

            if !Confirm::with_theme(&theme)
                .with_prompt("Add another file?")
                .default(false)
                .interact()?
            {
                break;
            }
        }
    }

    // Wait for completion?
    let wait = Confirm::with_theme(&theme)
        .with_prompt("Wait for completion?")
        .default(true)
        .interact()?;

    let timeout: u64 = if wait {
        Input::with_theme(&theme)
            .with_prompt("Timeout (seconds)")
            .default(300u64)
            .interact_text()?
    } else {
        300
    };

    // Run the model
    println!();
    println!("{}", style("Running model...").cyan().bold());

    let result = model_provider
        .run(&model, input, &files, wait, timeout)
        .await?;

    // Print result
    println!();
    print_result(&result, OutputFormat::Pretty, true)?;

    Ok(())
}
