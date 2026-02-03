# codex-rs

A CLI tool for interacting with AI models from third-party providers: Replicate, Fal, and WaveSpeed.

## Features

- **Multi-provider support**: Run models on Replicate, Fal, or WaveSpeed with a unified interface
- **File uploads**: Easily upload local files as model inputs
- **Interactive mode**: Guided model selection and input configuration
- **JSON input**: Provide model inputs as JSON (inline or from file)
- **Polling support**: Wait for predictions to complete with configurable timeout
- **Schema inspection**: Fetch model API schemas and documentation
- **Secure config**: Store API keys locally with masked display

## Installation

```bash
cargo install --path .
```

## Configuration

Set API keys via environment variables or the config command:

### Environment Variables

```bash
export REPLICATE_API_TOKEN="your-replicate-token"
export FAL_KEY="your-fal-key"
export WAVESPEED_API_KEY="your-wavespeed-key"
```

### Config Command

```bash
# Set API keys
codex-rs config set replicate your-api-key
codex-rs config set fal your-api-key
codex-rs config set wavespeed your-api-key

# Show current configuration
codex-rs config show

# Clear keys
codex-rs config clear replicate
codex-rs config clear  # clears all
```

## Usage

### Run a Model

```bash
# Basic usage with JSON input
codex-rs run -p replicate -m stability-ai/sdxl -i '{"prompt": "a photo of a cat"}' --wait

# With file upload
codex-rs run -p fal -m fal-ai/whisper -f audio=./speech.mp3 --wait

# From JSON file
codex-rs run -p wavespeed -m wavespeed/flux-dev --input-file params.json --wait

# Without waiting (returns prediction ID)
codex-rs run -p replicate -m meta/llama-2-70b-chat -i '{"prompt": "Hello!"}'
```

### Interactive Mode

```bash
# Fully interactive
codex-rs interactive

# With provider pre-selected
codex-rs interactive -p replicate
```

### Get Model Schema

```bash
codex-rs schema -p replicate -m stability-ai/sdxl
codex-rs schema -p fal -m fal-ai/flux/dev
```

### List Models

```bash
codex-rs list -p replicate
codex-rs list -p replicate -q "stable diffusion"
codex-rs list -p fal --limit 10
```

### Output Formats

```bash
# Default text output
codex-rs run -p replicate -m owner/model -i '{}' --wait

# JSON output
codex-rs run -p replicate -m owner/model -i '{}' --wait -o json

# Pretty-printed JSON
codex-rs run -p replicate -m owner/model -i '{}' --wait -o pretty

# Verbose output (includes metrics and URLs)
codex-rs run -p replicate -m owner/model -i '{}' --wait -v
```

## File Upload

Files can be uploaded using the `-f` flag with the format `param_name=path/to/file`:

```bash
# Single file
codex-rs run -p fal -m fal-ai/whisper -f audio=./recording.mp3 --wait

# Multiple files
codex-rs run -p replicate -m owner/model -f image=./photo.jpg -f mask=./mask.png --wait
```

Supported file types are automatically detected by extension:
- Images: jpg, jpeg, png, gif, webp
- Audio: mp3, wav
- Video: mp4, webm
- Documents: pdf, txt, json

## Provider-Specific Notes

### Replicate
- Model format: `owner/name` or `owner/name:version`
- Supports model listing and search
- Full schema introspection via OpenAPI

### Fal
- Model format: `fal-ai/model-name` or `owner/model`
- Uses queue-based execution
- File uploads via presigned URLs

### WaveSpeed
- Model format: `wavespeed/model-name`
- Similar API to Replicate

## License

MIT
