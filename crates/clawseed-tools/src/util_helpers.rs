use std::path::{Component, Path, PathBuf};

/// Truncate a string to `max_chars` Unicode characters, appending "..." if truncated.
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}...", s[..idx].trim_end()),
        None => s.to_string(),
    }
}

/// Utility enum for handling optional values in config set/unset operations.
pub enum MaybeSet<T> {
    Set(T),
    Unset,
    Null,
}

/// Validate a user-supplied path as relative to the workspace.
pub fn validate_workspace_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Path must not be empty.".into());
    }

    let mut relative = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err("Path traversal ('..') is not allowed.".into());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(
                    "Absolute paths are not allowed. Use a path relative to the workspace.".into(),
                );
            }
        }
    }

    if relative.as_os_str().is_empty() {
        return Err("Path must name a file inside the workspace.".into());
    }

    Ok(relative)
}

/// Resolve an existing path and ensure it stays inside the workspace.
pub fn resolve_existing_workspace_path(workspace: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = validate_workspace_relative_path(path)?;
    let workspace_canon = std::fs::canonicalize(workspace)
        .map_err(|e| format!("Cannot resolve workspace '{}': {e}", workspace.display()))?;
    let full_path = workspace_canon.join(relative);
    let canonical = std::fs::canonicalize(&full_path)
        .map_err(|e| format!("Cannot resolve path {path}: {e}"))?;

    if !canonical.starts_with(&workspace_canon) {
        return Err(format!("Path {path} is outside workspace"));
    }

    Ok(canonical)
}

/// Resolve a destination for writing. The target may not exist yet, but every
/// existing ancestor must remain inside the workspace after symlink resolution.
pub fn resolve_workspace_write_path(workspace: &Path, path: &str) -> Result<PathBuf, String> {
    let relative = validate_workspace_relative_path(path)?;
    let workspace_canon = std::fs::canonicalize(workspace)
        .map_err(|e| format!("Cannot resolve workspace '{}': {e}", workspace.display()))?;

    let mut current = workspace_canon.clone();
    for component in relative.components() {
        current.push(component.as_os_str());
        if current.exists() {
            let current_canon = std::fs::canonicalize(&current)
                .map_err(|e| format!("Cannot resolve path '{}': {e}", current.display()))?;
            if !current_canon.starts_with(&workspace_canon) {
                return Err(format!("Path {path} is outside workspace"));
            }
        }
    }

    Ok(workspace_canon.join(relative))
}
