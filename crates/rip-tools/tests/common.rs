use std::path::Path;

use rip_tools::{register_builtin_tools, BuiltinToolConfig, ToolRegistry};

pub fn setup_registry(root: &Path) -> ToolRegistry {
    let registry = ToolRegistry::default();
    let config = BuiltinToolConfig {
        workspace_root: root.to_path_buf(),
        max_bytes: 1024 * 1024,
        max_results: 100,
        max_depth: 16,
        follow_symlinks: false,
        include_hidden: false,
    };
    register_builtin_tools(&registry, config);
    registry
}
