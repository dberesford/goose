use goose_mcp_lite::BUILTIN_EXTENSIONS;
use std::collections::HashSet;

#[test]
fn builtin_extensions_are_developer_only() {
    let names: HashSet<&str> = BUILTIN_EXTENSIONS.keys().copied().collect();
    assert_eq!(names.len(), 1);
    assert!(names.contains("developer"));
}
