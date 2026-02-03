mod cli;
mod config;
mod interactive;
mod output;
mod providers;

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Commands, ConfigAction, Provider};
use config::{get_api_key_for_provider, Config};
use output::{print_models, print_result, print_schema};
use providers::{
    fal::FalProvider, parse_file_args, replicate::ReplicateProvider,
    wavespeed::WaveSpeedProvider, ModelProvider,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            provider,
            model,
            input,
            input_file,
            file,
            wait,
            timeout,
        } => {
            run_model(
                provider,
                &model,
                input,
                input_file,
                file,
                wait,
                timeout,
                cli.output,
                cli.verbose,
            )
            .await?;
        }

        Commands::Schema { provider, model } => {
            fetch_schema(provider, &model, cli.output).await?;
        }

        Commands::List {
            provider,
            query,
            limit,
        } => {
            list_models(provider, query.as_deref(), limit, cli.output).await?;
        }

        Commands::Interactive { provider } => {
            interactive::run_interactive(provider).await?;
        }

        Commands::Config { action } => {
            handle_config(action)?;
        }
    }

    Ok(())
}

async fn run_model(
    provider: Provider,
    model: &str,
    input: Option<String>,
    input_file: Option<std::path::PathBuf>,
    file_args: Vec<String>,
    wait: bool,
    timeout: u64,
    output_format: cli::OutputFormat,
    verbose: bool,
) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let api_key = get_api_key_for_provider(provider, &config)?;

    let model_provider = create_provider(provider, api_key)?;

    // Parse input JSON
    let input_value = if let Some(input_str) = input {
        serde_json::from_str(&input_str).context("Invalid input JSON")?
    } else if let Some(path) = input_file {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read input file: {}", path.display()))?;
        serde_json::from_str(&content).context("Invalid JSON in input file")?
    } else {
        serde_json::json!({})
    };

    // Parse file arguments
    let files = parse_file_args(&file_args)?;

    // Run the model
    let result = model_provider
        .run(model, input_value, &files, wait, timeout)
        .await?;

    print_result(&result, output_format, verbose)?;

    Ok(())
}

async fn fetch_schema(
    provider: Provider,
    model: &str,
    output_format: cli::OutputFormat,
) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let api_key = get_api_key_for_provider(provider, &config)?;

    let model_provider = create_provider(provider, api_key)?;
    let schema = model_provider.get_schema(model).await?;

    print_schema(&schema, output_format)?;

    Ok(())
}

async fn list_models(
    provider: Provider,
    query: Option<&str>,
    limit: usize,
    output_format: cli::OutputFormat,
) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let api_key = get_api_key_for_provider(provider, &config)?;

    let model_provider = create_provider(provider, api_key)?;
    let models = model_provider.list_models(query, limit).await?;

    print_models(&models, output_format)?;

    Ok(())
}

fn handle_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Set { provider, key } => {
            let mut config = Config::load().unwrap_or_default();
            config.set_api_key(provider, key);
            config.save()?;
            println!("API key for {} saved successfully.", provider);
        }

        ConfigAction::Show => {
            let config = Config::load().unwrap_or_default();
            let path = Config::config_path()?;
            println!("Config file: {}", path.display());
            println!();

            if config.api_keys.is_empty() {
                println!("No API keys configured.");
            } else {
                println!("Configured API keys:");
                for (provider, key) in &config.api_keys {
                    // Mask the key for security
                    let masked = if key.len() > 8 {
                        format!("{}...{}", &key[..4], &key[key.len() - 4..])
                    } else {
                        "****".to_string()
                    };
                    println!("  {}: {}", provider, masked);
                }
            }

            println!();
            println!("Environment variables (if set, take precedence):");
            println!("  REPLICATE_API_TOKEN: {}",
                if std::env::var("REPLICATE_API_TOKEN").is_ok() { "set" } else { "not set" });
            println!("  FAL_KEY: {}",
                if std::env::var("FAL_KEY").is_ok() || std::env::var("FAL_API_KEY").is_ok() { "set" } else { "not set" });
            println!("  WAVESPEED_API_KEY: {}",
                if std::env::var("WAVESPEED_API_KEY").is_ok() { "set" } else { "not set" });
        }

        ConfigAction::Clear { provider } => {
            let mut config = Config::load().unwrap_or_default();
            if let Some(p) = provider {
                config.clear_api_key(p);
                println!("API key for {} cleared.", p);
            } else {
                config.clear_all();
                println!("All API keys cleared.");
            }
            config.save()?;
        }
    }

    Ok(())
}

fn create_provider(provider: Provider, api_key: String) -> Result<Box<dyn ModelProvider>> {
    match provider {
        Provider::Replicate => Ok(Box::new(ReplicateProvider::new(api_key)?)),
        Provider::Fal => Ok(Box::new(FalProvider::new(api_key)?)),
        Provider::WaveSpeed => Ok(Box::new(WaveSpeedProvider::new(api_key)?)),
    }
}
