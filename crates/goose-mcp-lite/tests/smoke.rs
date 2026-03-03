use goose_mcp_lite::BUILTIN_EXTENSIONS;
use std::collections::HashSet;

#[test]
fn builtin_extensions_include_developer() {
    let names: HashSet<&str> = BUILTIN_EXTENSIONS.keys().copied().collect();
    // The lite feature only requests "builtin-developer", but Cargo feature
    // unification in workspace builds may activate additional builtins.
    assert!(names.contains("developer"));
}
