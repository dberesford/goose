# Goose Feature Profiles

## Build profiles

- **Full (default)**: `cargo build -p goose`
- **Lite**: `cargo build -p goose --no-default-features --features lite`

## Feature groups

- `providers-aws`: enables AWS Bedrock and SageMaker provider support.
- `providers-gcp-vertex`: enables GCP Vertex provider support.
- `dictation`: enables local Whisper dictation stack.
- `telemetry`: enables OpenTelemetry tracing/metrics/logging support.
- `analytics`: enables PostHog analytics event transport.
- `recipes`: reserved for future recipe-specific gating.
- `scheduler`: reserved for future scheduler-specific gating.
- `mcp-full`: reserved for future full MCP profile wiring.

## Lite provider baseline

With `--no-default-features --features lite`, the built-in provider registry includes:

- `openai`
- `anthropic`
- `openrouter`
- `ollama`
- `litellm`

Unknown providers return an error that includes:

- `"may be disabled in this build profile"`
