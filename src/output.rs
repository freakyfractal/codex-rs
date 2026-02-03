use anyhow::Result;
use console::style;
use serde_json::Value;

use crate::cli::OutputFormat;
use crate::providers::{ModelInfo, ModelSchema, PredictionResult, PredictionStatus};

pub fn print_result(result: &PredictionResult, format: OutputFormat, verbose: bool) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(result)?);
        }
        OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(result)?);
        }
        OutputFormat::Text => {
            print_result_text(result, verbose);
        }
    }
    Ok(())
}

fn print_result_text(result: &PredictionResult, verbose: bool) {
    // Status with color
    let status_str = match result.status {
        PredictionStatus::Starting => style("starting").yellow(),
        PredictionStatus::Processing => style("processing").yellow(),
        PredictionStatus::Succeeded => style("succeeded").green(),
        PredictionStatus::Failed => style("failed").red(),
        PredictionStatus::Canceled => style("canceled").dim(),
    };

    println!("{} {}", style("Status:").bold(), status_str);
    println!("{} {}", style("ID:").bold(), result.id);

    if let Some(ref error) = result.error {
        println!("{} {}", style("Error:").red().bold(), error);
    }

    if let Some(ref output) = result.output {
        println!("{}", style("Output:").bold());
        print_value(output, 2);
    }

    if verbose {
        if let Some(ref metrics) = result.metrics {
            println!();
            println!("{}", style("Metrics:").bold());
            if let Some(predict_time) = metrics.predict_time {
                println!("  Predict time: {:.2}s", predict_time);
            }
            if let Some(total_time) = metrics.total_time {
                println!("  Total time: {:.2}s", total_time);
            }
        }

        if let Some(ref urls) = result.urls {
            println!();
            println!("{}", style("URLs:").bold());
            if let Some(ref get_url) = urls.get {
                println!("  Get: {}", get_url);
            }
            if let Some(ref cancel_url) = urls.cancel {
                println!("  Cancel: {}", cancel_url);
            }
        }
    }
}

pub fn print_schema(schema: &ModelSchema, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(schema)?);
        }
        OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(schema)?);
        }
        OutputFormat::Text => {
            println!("{} {}", style("Model:").bold(), schema.name);
            if let Some(ref desc) = schema.description {
                println!("{} {}", style("Description:").bold(), desc);
            }
            println!();
            println!("{}", style("Input Schema:").bold());
            println!("{}", serde_json::to_string_pretty(&schema.input_schema)?);
            if let Some(ref output_schema) = schema.output_schema {
                println!();
                println!("{}", style("Output Schema:").bold());
                println!("{}", serde_json::to_string_pretty(output_schema)?);
            }
        }
    }
    Ok(())
}

pub fn print_models(models: &[ModelInfo], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(models)?);
        }
        OutputFormat::Pretty => {
            println!("{}", serde_json::to_string_pretty(models)?);
        }
        OutputFormat::Text => {
            if models.is_empty() {
                println!("{}", style("No models found.").dim());
                return Ok(());
            }

            for model in models {
                println!("{}", style(&model.id).green().bold());
                if !model.name.is_empty() && model.name != model.id {
                    println!("  Name: {}", model.name);
                }
                if let Some(ref desc) = model.description {
                    let short_desc = if desc.len() > 80 {
                        format!("{}...", &desc[..77])
                    } else {
                        desc.clone()
                    };
                    println!("  {}", style(short_desc).dim());
                }
                if let Some(ref owner) = model.owner {
                    println!("  Owner: {}", owner);
                }
                if let Some(ref url) = model.url {
                    println!("  URL: {}", style(url).blue().underlined());
                }
                println!();
            }
        }
    }
    Ok(())
}

fn print_value(value: &Value, indent: usize) {
    let prefix = " ".repeat(indent);
    match value {
        Value::Null => println!("{}null", prefix),
        Value::Bool(b) => println!("{}{}", prefix, b),
        Value::Number(n) => println!("{}{}", prefix, n),
        Value::String(s) => {
            // Check if it's a URL (likely an output file)
            if s.starts_with("http://") || s.starts_with("https://") {
                println!("{}{}", prefix, style(s).blue().underlined());
            } else if s.len() > 100 {
                // Truncate long strings
                println!("{}{}...", prefix, &s[..97]);
            } else {
                println!("{}{}", prefix, s);
            }
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                if arr.len() > 1 {
                    println!("{}[{}]:", prefix, i);
                }
                print_value(item, indent + 2);
            }
        }
        Value::Object(obj) => {
            for (key, val) in obj {
                print!("{}{}: ", prefix, style(key).cyan());
                match val {
                    Value::Object(_) | Value::Array(_) => {
                        println!();
                        print_value(val, indent + 2);
                    }
                    _ => {
                        // Print simple values on same line
                        print_value(val, 0);
                    }
                }
            }
        }
    }
}
