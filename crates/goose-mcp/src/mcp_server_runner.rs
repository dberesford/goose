use std::str::FromStr;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

#[derive(Clone, Debug)]
pub enum McpCommand {
    #[cfg(feature = "builtin-autovisualiser")]
    AutoVisualiser,
    #[cfg(feature = "builtin-computercontroller")]
    ComputerController,
    #[cfg(feature = "builtin-developer")]
    Developer,
    #[cfg(feature = "builtin-memory")]
    Memory,
    #[cfg(feature = "builtin-tutorial")]
    Tutorial,
}

impl FromStr for McpCommand {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().replace(' ', "").as_str() {
            #[cfg(feature = "builtin-autovisualiser")]
            "autovisualiser" => Ok(McpCommand::AutoVisualiser),
            #[cfg(feature = "builtin-computercontroller")]
            "computercontroller" => Ok(McpCommand::ComputerController),
            #[cfg(feature = "builtin-developer")]
            "developer" => Ok(McpCommand::Developer),
            #[cfg(feature = "builtin-memory")]
            "memory" => Ok(McpCommand::Memory),
            #[cfg(feature = "builtin-tutorial")]
            "tutorial" => Ok(McpCommand::Tutorial),
            _ => Err(format!("Invalid command: {}", s)),
        }
    }
}

impl McpCommand {
    pub fn name(&self) -> &str {
        match self {
            #[cfg(feature = "builtin-autovisualiser")]
            McpCommand::AutoVisualiser => "autovisualiser",
            #[cfg(feature = "builtin-computercontroller")]
            McpCommand::ComputerController => "computercontroller",
            #[cfg(feature = "builtin-developer")]
            McpCommand::Developer => "developer",
            #[cfg(feature = "builtin-memory")]
            McpCommand::Memory => "memory",
            #[cfg(feature = "builtin-tutorial")]
            McpCommand::Tutorial => "tutorial",
        }
    }
}

pub async fn serve<S>(server: S) -> Result<()>
where
    S: rmcp::ServerHandler,
{
    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;

    service.waiting().await?;

    Ok(())
}
