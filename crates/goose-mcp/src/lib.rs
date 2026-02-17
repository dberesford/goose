use etcetera::AppStrategyArgs;
use once_cell::sync::Lazy;
use rmcp::{ServerHandler, ServiceExt};
use std::collections::HashMap;

pub static APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "goose".to_string(),
});

#[cfg(feature = "builtin-autovisualiser")]
pub mod autovisualiser;
#[cfg(feature = "builtin-computercontroller")]
pub mod computercontroller;
#[cfg(feature = "builtin-developer")]
pub mod developer;
pub mod mcp_server_runner;
#[cfg(feature = "builtin-memory")]
mod memory;
pub mod subprocess;
#[cfg(feature = "builtin-tutorial")]
pub mod tutorial;

#[cfg(feature = "builtin-autovisualiser")]
pub use autovisualiser::AutoVisualiserRouter;
#[cfg(feature = "builtin-computercontroller")]
pub use computercontroller::ComputerControllerServer;
#[cfg(feature = "builtin-developer")]
pub use developer::rmcp_developer::DeveloperServer;
#[cfg(feature = "builtin-memory")]
pub use memory::MemoryServer;
#[cfg(feature = "builtin-tutorial")]
pub use tutorial::TutorialServer;

/// Type definition for a function that spawns and serves a builtin extension server
pub type SpawnServerFn = fn(tokio::io::DuplexStream, tokio::io::DuplexStream);

fn spawn_and_serve<S>(
    name: &'static str,
    server: S,
    transport: (tokio::io::DuplexStream, tokio::io::DuplexStream),
) where
    S: ServerHandler + Send + 'static,
{
    tokio::spawn(async move {
        match server.serve(transport).await {
            Ok(running) => {
                let _ = running.waiting().await;
            }
            Err(e) => tracing::error!(builtin = name, error = %e, "server error"),
        }
    });
}

macro_rules! builtin {
    ($name:ident, $server_ty:ty) => {{
        fn spawn(r: tokio::io::DuplexStream, w: tokio::io::DuplexStream) {
            spawn_and_serve(stringify!($name), <$server_ty>::new(), (r, w));
        }
        (stringify!($name), spawn as SpawnServerFn)
    }};
}

pub static BUILTIN_EXTENSIONS: Lazy<HashMap<&'static str, SpawnServerFn>> = Lazy::new(|| {
    let mut extensions = HashMap::new();

    #[cfg(feature = "builtin-developer")]
    {
        let (name, spawn) = builtin!(developer, DeveloperServer);
        extensions.insert(name, spawn);
    }
    #[cfg(feature = "builtin-autovisualiser")]
    {
        let (name, spawn) = builtin!(autovisualiser, AutoVisualiserRouter);
        extensions.insert(name, spawn);
    }
    #[cfg(feature = "builtin-computercontroller")]
    {
        let (name, spawn) = builtin!(computercontroller, ComputerControllerServer);
        extensions.insert(name, spawn);
    }
    #[cfg(feature = "builtin-memory")]
    {
        let (name, spawn) = builtin!(memory, MemoryServer);
        extensions.insert(name, spawn);
    }
    #[cfg(feature = "builtin-tutorial")]
    {
        let (name, spawn) = builtin!(tutorial, TutorialServer);
        extensions.insert(name, spawn);
    }

    extensions
});
