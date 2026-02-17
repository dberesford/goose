# goose-lite

`goose-lite` is a minimal import target for embedding Goose with a smaller feature footprint.

## Profile

- Depends on `goose` with `default-features = false` and `features = ["lite"]`.
- Uses `goose-mcp-lite` for builtin MCP registration.

## Public API contract

The crate intentionally re-exports a narrow subset:

- `agents`: `Agent`, `AgentEvent`, `ExtensionConfig`, `SessionConfig`
- `config`: `Config`, `get_enabled_extensions`, `DEFAULT_*`, `paths::Paths`
- `conversation::message`: `Message`, `MessageContent`
- `model`: `ModelConfig`
- `providers`: `create`, `providers`
- `session`: `EnabledExtensionsState`, `session_manager::{SessionManager, SessionType}`
- `builtin_extension`: registration APIs and `BUILTIN_EXTENSIONS` from `goose-mcp-lite`

This explicit surface is intended to remain stable for downstream integrations.
