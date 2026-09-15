use std::path::{Path, PathBuf};

/// Resolve a model-supplied path against the project root.
///
/// Models routinely emit paths that *look* absolute but mean "relative to the
/// project" — `/src/lib.rs`, `./src/lib.rs`, `src/lib.rs`. Naively joining
/// those with `Path::join` silently discards the project root (`/src/lib.rs`
/// wins outright), producing a path outside the sandbox and a confusing
/// failure. Normalising here converts a recurring class of model mistake into
/// correct behaviour.
///
/// This does **not** weaken the sandbox: the resolved path is still validated
/// against the allowed scope afterwards.
pub fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return cwd.to_path_buf();
    }

    // A path that is already absolute *and* inside the project is fine.
    let as_path = Path::new(trimmed);
    if as_path.is_absolute() && as_path.starts_with(cwd) {
        return as_path.to_path_buf();
    }

    // Otherwise treat it as relative to the project root, stripping the
    // leading separator and any redundant `./` prefix.
    let relative = trimmed.trim_start_matches("./").trim_start_matches('/');
    if relative.is_empty() {
        return cwd.to_path_buf();
    }

    cwd.join(relative)
}

/// Read the first present key from a set of aliases.
///
/// Models vary in what they call a path argument (`path`, `file`,
/// `file_path`, ...). Accepting the common spellings avoids a whole category
/// of avoidable failures.
pub fn string_arg<'a>(
    args: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a str> {
    keys.iter().find_map(|k| args.get(*k).and_then(|v| v.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_leading_slash() {
        let cwd = Path::new("/project");
        assert_eq!(resolve_path(cwd, "/src/lib.rs"), PathBuf::from("/project/src/lib.rs"));
    }

    #[test]
    fn strips_dot_slash() {
        let cwd = Path::new("/project");
        assert_eq!(resolve_path(cwd, "./src/lib.rs"), PathBuf::from("/project/src/lib.rs"));
    }

    #[test]
    fn keeps_plain_relative() {
        let cwd = Path::new("/project");
        assert_eq!(resolve_path(cwd, "src/lib.rs"), PathBuf::from("/project/src/lib.rs"));
    }

    #[test]
    fn keeps_absolute_paths_inside_the_project() {
        let cwd = Path::new("/project");
        assert_eq!(
            resolve_path(cwd, "/project/src/lib.rs"),
            PathBuf::from("/project/src/lib.rs")
        );
    }

    #[test]
    fn empty_path_is_the_project_root() {
        let cwd = Path::new("/project");
        assert_eq!(resolve_path(cwd, ""), PathBuf::from("/project"));
        assert_eq!(resolve_path(cwd, "/"), PathBuf::from("/project"));
    }
}
