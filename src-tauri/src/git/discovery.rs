use super::*;
use std::{
    collections::{HashSet, VecDeque},
    fs::Metadata,
    time::{Duration, Instant},
};
use tokio::task::JoinSet;

#[derive(Clone, Copy)]
struct Limits {
    directories: usize,
    candidates: usize,
    issues: usize,
    entries: usize,
    elapsed: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            directories: 10_000,
            candidates: 1_000,
            issues: 200,
            entries: 100_000,
            elapsed: Duration::from_secs(30),
        }
    }
}
#[derive(Default)]
struct Inspection {
    marker: bool,
    children: Vec<PathBuf>,
    issues: Vec<DiscoveryIssue>,
    truncated: bool,
}
fn display(path: &Path) -> String {
    dunce::simplified(path).to_string_lossy().into_owned()
}
fn issue(path: &Path, reason: impl Into<String>) -> DiscoveryIssue {
    DiscoveryIssue {
        path: display(path),
        reason: reason.into(),
    }
}
fn link_or_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
fn skipped(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            ".git"
                | ".hg"
                | ".svn"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".venv"
                | "venv"
                | ".cache"
                | ".next"
                | ".nuxt"
                | ".tox"
                | "__pycache__"
                | ".gradle"
                | "vendor"
                | "coverage"
        )
    })
}
fn inspect(
    path: &Path,
    descend: bool,
    check_marker: bool,
    deadline: Instant,
    limits: Limits,
) -> Inspection {
    let mut result = Inspection::default();
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) => {
            result
                .issues
                .push(issue(path, format!("Cannot inspect folder: {error}")));
            return result;
        }
    };
    if link_or_reparse(&metadata) {
        result
            .issues
            .push(issue(path, "Skipped a symbolic link or reparse point."));
        return result;
    }
    if !metadata.is_dir() {
        result
            .issues
            .push(issue(path, "The selected path is not a directory."));
        return result;
    }
    if check_marker {
        let marker = path.join(".git");
        match std::fs::symlink_metadata(&marker) {
            Ok(metadata) if link_or_reparse(&metadata) => result.issues.push(issue(
                &marker,
                "Skipped a Git marker that is a symbolic link or reparse point.",
            )),
            Ok(metadata) if metadata.is_file() || metadata.is_dir() => {
                result.marker = true;
                return result;
            }
            Ok(_) => result.issues.push(issue(
                &marker,
                "Git marker is neither a regular file nor a directory.",
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => result
                .issues
                .push(issue(path, format!("Cannot inspect Git marker: {error}"))),
        }
    }
    if !descend {
        return result;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            result
                .issues
                .push(issue(path, format!("Cannot read folder: {error}")));
            return result;
        }
    };
    for (count, entry) in entries.enumerate() {
        if Instant::now() >= deadline
            || count >= limits.entries
            || result.children.len() >= limits.directories
            || result.issues.len() >= limits.issues
        {
            result.truncated = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result
                    .issues
                    .push(issue(path, format!("Cannot read folder entry: {error}")));
                continue;
            }
        };
        if skipped(&entry.file_name()) {
            continue;
        }
        let child = entry.path();
        let metadata = match std::fs::symlink_metadata(&child) {
            Ok(value) => value,
            Err(error) => {
                result
                    .issues
                    .push(issue(&child, format!("Cannot inspect entry: {error}")));
                continue;
            }
        };
        if link_or_reparse(&metadata) {
            result
                .issues
                .push(issue(&child, "Skipped a symbolic link or reparse point."));
            continue;
        }
        if metadata.is_dir() {
            result.children.push(child);
        }
    }
    result.children.sort();
    result
}
impl GitEngine {
    pub async fn discover_repositories(
        &self,
        root: &Path,
        depth: u8,
    ) -> AppResult<DiscoveryResult> {
        self.discover_with_limits(root, depth, Limits::default())
            .await
    }
    async fn discover_with_limits(
        &self,
        root: &Path,
        depth: u8,
        limits: Limits,
    ) -> AppResult<DiscoveryResult> {
        if depth > 10 {
            return Err(AppError::new(
                "invalid_depth",
                "Discovery depth must be between 0 and 10.",
            ));
        }
        let root = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|error| AppError::new("discovery_unavailable", error.to_string()))?
                .join(root)
        };
        let deadline = Instant::now() + limits.elapsed;
        let mut result = DiscoveryResult {
            root_path: display(&root),
            depth,
            candidates: Vec::new(),
            issues: Vec::new(),
            truncated: false,
            visited_directories: 0,
        };
        let mut queue = VecDeque::from([(root.clone(), 0_u8)]);
        let mut pending = JoinSet::new();
        let mut identities = HashSet::new();
        loop {
            while pending.len() < 4
                && !queue.is_empty()
                && result.visited_directories < limits.directories
                && result.candidates.len() < limits.candidates
                && Instant::now() < deadline
            {
                let (path, level) = queue.pop_front().unwrap();
                let engine = self.clone();
                result.visited_directories += 1;
                pending.spawn(async move {
                    let inspect_path = path.clone();
                    let mut inspected = tokio::task::spawn_blocking(move || {
                        inspect(&inspect_path, level < depth, true, deadline, limits)
                    })
                    .await
                    .map_err(|error| AppError::new("discovery_failed", error.to_string()))?;
                    let mut candidate = None;
                    if inspected.marker {
                        match engine.register_repository(&path).await {
                            Ok(opened) => {
                                let canonical = &opened.root;
                                let label = canonical
                                    .file_name()
                                    .unwrap_or(canonical.as_os_str())
                                    .to_string_lossy()
                                    .into_owned();
                                if let Some(action_path) = dunce::simplified(canonical).to_str() {
                                    candidate = Some(DiscoveryCandidate {
                                        repository_id: opened.id,
                                        display_path: action_path.to_owned(),
                                        label,
                                    });
                                } else {
                                    inspected.issues.push(issue(canonical, "Repository path is not UTF-8 and cannot safely be saved in a project file; this candidate was omitted."));
                                }
                            }
                            Err(error) => {
                                inspected.issues.push(issue(
                                    &path,
                                    format!(
                                        "Cannot open Git repository: {} ({})",
                                        error.message, error.code
                                    ),
                                ));
                                // A malformed marker must not hide valid nested repositories.
                                let inspect_path = path.clone();
                                let nested = tokio::task::spawn_blocking(move || {
                                    inspect(&inspect_path, level < depth, false, deadline, limits)
                                })
                                .await
                                .map_err(|error| {
                                    AppError::new("discovery_failed", error.to_string())
                                })?;
                                inspected.children = nested.children;
                                inspected.issues.extend(nested.issues);
                                inspected.truncated |= nested.truncated;
                            }
                        }
                    }
                    Ok::<_, AppError>((level, inspected, candidate))
                });
            }
            if pending.is_empty() {
                if !queue.is_empty() {
                    result.truncated = true;
                }
                break;
            }
            let next = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                pending.join_next(),
            )
            .await;
            let (level, inspected, candidate) = match next {
                Ok(Some(Ok(Ok(value)))) => value,
                Ok(Some(Ok(Err(error)))) => {
                    result.issues.push(issue(&root, error.message));
                    result.truncated = true;
                    break;
                }
                Ok(Some(Err(error))) => {
                    result
                        .issues
                        .push(issue(&root, format!("Discovery worker stopped: {error}")));
                    result.truncated = true;
                    break;
                }
                _ => {
                    result.truncated = true;
                    break;
                }
            };
            result.truncated |= inspected.truncated;
            for item in inspected.issues {
                if result.issues.len() == limits.issues {
                    result.truncated = true;
                    break;
                }
                result.issues.push(item);
            }
            if let Some(candidate) = candidate {
                if identities.insert(candidate.repository_id.clone()) {
                    if result.candidates.len() == limits.candidates {
                        result.truncated = true;
                        break;
                    }
                    result.candidates.push(candidate);
                }
            } else {
                for child in inspected.children {
                    if result.visited_directories + queue.len() >= limits.directories {
                        result.truncated = true;
                        break;
                    }
                    queue.push_back((child, level + 1));
                }
            }
            if result.candidates.len() >= limits.candidates
                && (!pending.is_empty() || !queue.is_empty())
            {
                result.truncated = true;
                break;
            }
        }
        pending.abort_all();
        if result.truncated {
            if result.issues.len() >= limits.issues {
                result.issues.truncate(limits.issues.saturating_sub(1));
            }
            result.issues.push(issue(&root,"Discovery returned partial results after reaching a time, directory, candidate, entry, or issue limit. Narrow the folder or depth and scan again."));
        }
        result
            .candidates
            .sort_by(|a, b| a.display_path.cmp(&b.display_path));
        result
            .issues
            .sort_by(|a, b| a.path.cmp(&b.path).then(a.reason.cmp(&b.reason)));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn empty_repository(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        let engine = GitEngine::isolated();
        runner::checked(
            engine
                .runner
                .run(
                    path,
                    &runner::args(&["init", "--template=", "-b", "main"]),
                    None,
                    true,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await
                .unwrap(),
        )
        .unwrap();
    }
    #[tokio::test]
    async fn directory_candidate_and_time_limits_report_partial_results() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            empty_repository(&dir.path().join(name)).await;
        }
        let engine = GitEngine::isolated();
        let directories = engine
            .discover_with_limits(
                dir.path(),
                3,
                Limits {
                    directories: 2,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        assert!(directories.truncated);
        assert!(directories.visited_directories <= 2);
        assert!(directories.candidates.len() <= 1);
        assert!(!directories.issues.is_empty());
        let candidates = engine
            .discover_with_limits(
                dir.path(),
                3,
                Limits {
                    candidates: 1,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        assert!(candidates.truncated);
        assert_eq!(candidates.candidates.len(), 1);
        let timed = engine
            .discover_with_limits(
                dir.path(),
                3,
                Limits {
                    elapsed: Duration::ZERO,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        assert!(timed.truncated);
        assert_eq!(timed.visited_directories, 0);
        assert!(timed.candidates.is_empty());
        assert!(!timed.issues.is_empty());
    }
    #[tokio::test]
    async fn issue_and_entry_limits_are_bounded_and_explicit() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            let path = dir.path().join(name);
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join(".git"), "invalid marker").unwrap();
        }
        let engine = GitEngine::isolated();
        let result = engine
            .discover_with_limits(
                dir.path(),
                3,
                Limits {
                    issues: 2,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        assert!(result.truncated);
        assert_eq!(result.issues.len(), 2);
        assert!(result
            .issues
            .iter()
            .any(|item| item.reason.contains("partial results")));
        let entries = engine
            .discover_with_limits(
                dir.path(),
                3,
                Limits {
                    entries: 1,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        assert!(entries.truncated);
        assert!(entries.visited_directories <= 2);
    }
}
