use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codex-rs")]
#[command(author, version, about = "CLI tool for AI model inference via Replicate, Fal, and WaveSpeed")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format
    #[arg(long, short, global = true, default_value = "text")]
    pub output: OutputFormat,

    /// Verbose output
    #[arg(long, short, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a model with the specified provider
    Run {
        /// The provider to use
        #[arg(long, short)]
        provider: Provider,

        /// The model identifier (e.g., "stability-ai/sdxl" for Replicate)
        #[arg(long, short)]
        model: String,

        /// Input as JSON string
        #[arg(long, short)]
        input: Option<String>,

        /// Input from a JSON file
        #[arg(long)]
        input_file: Option<PathBuf>,

        /// Files to upload (can be specified multiple times)
        /// Format: param_name=path/to/file
        #[arg(long, short = 'f')]
        file: Vec<String>,

        /// Wait for the prediction to complete (polling mode)
        #[arg(long, short)]
        wait: bool,

        /// Timeout in seconds when waiting
        #[arg(long, default_value = "300")]
        timeout: u64,
    },

    /// Fetch model schema/API information
    Schema {
        /// The provider to use
        #[arg(long, short)]
        provider: Provider,

        /// The model identifier
        #[arg(long, short)]
        model: String,
    },

    /// List available models (where supported)
    List {
        /// The provider to use
        #[arg(long, short)]
        provider: Provider,

        /// Search query
        #[arg(long, short)]
        query: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Interactive mode - guided model selection and input
    Interactive {
        /// The provider to use (optional, will prompt if not specified)
        #[arg(long, short)]
        provider: Option<Provider>,
    },

    /// Configure API keys and settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set an API key for a provider
    Set {
        /// The provider to configure
        provider: Provider,

        /// The API key
        key: String,
    },

    /// Show current configuration
    Show,

    /// Clear configuration
    Clear {
        /// The provider to clear (clears all if not specified)
        provider: Option<Provider>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum Provider {
    Replicate,
    Fal,
    WaveSpeed,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Replicate => write!(f, "replicate"),
            Provider::Fal => write!(f, "fal"),
            Provider::WaveSpeed => write!(f, "wavespeed"),
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Pretty,
}
