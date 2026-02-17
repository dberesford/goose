//! Minimal Goose surface for downstream embedding.
//!
//! This crate intentionally exposes a narrow, stable API over `goose` in lite mode,
//! plus the developer-only builtin MCP map from `goose-mcp-lite`.

pub mod agents {
    pub use goose::agents::{Agent, AgentEvent, ExtensionConfig, SessionConfig};
}

pub mod config {
    pub use goose::config::paths::Paths;
    pub use goose::config::{
        get_enabled_extensions, Config, DEFAULT_DISPLAY_NAME, DEFAULT_EXTENSION,
        DEFAULT_EXTENSION_DESCRIPTION, DEFAULT_EXTENSION_TIMEOUT,
    };
}

pub mod conversation {
    pub mod message {
        pub use goose::conversation::message::{Message, MessageContent};
    }
}

pub mod model {
    pub use goose::model::ModelConfig;
}

pub mod providers {
    pub use goose::providers::{create, providers};
}

pub mod session {
    pub use goose::session::EnabledExtensionsState;
    pub mod session_manager {
        pub use goose::session::session_manager::{SessionManager, SessionType};
    }
}

pub mod builtin_extension {
    pub use goose::builtin_extension::{
        get_builtin_extension, register_builtin_extension, register_builtin_extensions,
        SpawnServerFn,
    };
    pub use goose_mcp_lite::{DeveloperServer, BUILTIN_EXTENSIONS};
}
