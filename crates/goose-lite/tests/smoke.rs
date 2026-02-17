use goose_lite::{agents, builtin_extension, config, model, providers, session};
use std::collections::HashSet;

#[test]
fn reexports_are_accessible() {
    let _ = std::mem::size_of::<agents::SessionConfig>();
    let _ = std::mem::size_of::<config::Config>();
    let _ = std::mem::size_of::<model::ModelConfig>();
    let _ = std::mem::size_of::<session::EnabledExtensionsState>();

    let _providers_fn = providers::providers;
    let _create_fn = providers::create;
    let _register_fn = builtin_extension::register_builtin_extensions;

    assert!(builtin_extension::BUILTIN_EXTENSIONS.contains_key("developer"));
}

#[test]
fn builtin_registry_contains_only_developer_in_lite_profile() {
    let names: HashSet<&str> = builtin_extension::BUILTIN_EXTENSIONS
        .keys()
        .copied()
        .collect();
    assert_eq!(names.len(), 1);
    assert!(names.contains("developer"));
}

#[tokio::test]
async fn providers_match_lite_allowlist_baseline() {
    let names: HashSet<String> = providers::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| metadata.name)
        .collect();

    for expected in ["anthropic", "litellm", "ollama", "openai", "openrouter"] {
        assert!(names.contains(expected), "missing provider '{expected}'");
    }

    for excluded in [
        "google",
        "databricks",
        "chatgpt_codex",
        "claude_code",
        "codex",
    ] {
        assert!(
            !names.contains(excluded),
            "provider '{excluded}' should not be present in lite profile"
        );
    }
}
