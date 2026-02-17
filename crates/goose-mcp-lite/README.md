# goose-mcp-lite

`goose-mcp-lite` provides a lightweight MCP surface for downstream embedding.

## Scope

- Includes only the `developer` builtin server.
- Re-exports:
  - `DeveloperServer`
  - `BUILTIN_EXTENSIONS`
  - `SpawnServerFn`

## Excluded from lite profile

- `autovisualiser`
- `computercontroller`
- `memory`
- `tutorial`

This crate depends on `goose-mcp` with `default-features = false` and `features = ["lite"]`.
