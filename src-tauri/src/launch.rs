use crate::contracts::*;
use std::{collections::VecDeque, path::Path, sync::Mutex};

#[derive(Default)]
pub struct LaunchQueue(Mutex<VecDeque<LaunchRequest>>);
impl LaunchQueue {
    pub fn push(&self, request: LaunchRequest) {
        self.0.lock().unwrap().push_back(request);
    }
    pub fn next(&self) -> Option<LaunchRequest> {
        self.0.lock().unwrap().front().cloned()
    }
    pub fn ack(&self, id: &str) {
        let mut queue = self.0.lock().unwrap();
        if queue.front().is_some_and(|r| r.request_id == id) {
            queue.pop_front();
        }
    }
}
pub const HELP: &str = "Git View\n\nUsage:\n  git-view [path]\n  git-view repo [path]\n  git-view group <project.json>\n  git-view group --scan <folder> [--depth <0..10>]\n  git-view --help\n  git-view --version\n\nScan depth defaults to 3; the selected folder is depth 0. Review scan results before saving a group.\nPaths resolve from the invoking directory. No launch action mutates Git.\n";
pub fn parse(args: &[String], cwd: &Path) -> AppResult<LaunchRequest> {
    let scan_depth = match args {
        [mode, scan, root] if mode == "group" && scan == "--scan" && !root.starts_with('-') => {
            Some(3)
        }
        [mode, scan, root, flag, depth]
            if mode == "group"
                && scan == "--scan"
                && flag == "--depth"
                && !root.starts_with('-') =>
        {
            Some(
                depth
                    .parse::<u8>()
                    .ok()
                    .filter(|n| *n <= 10)
                    .ok_or_else(|| {
                        AppError::new(
                            "invalid_arguments",
                            "Scan depth must be an integer from 0 to 10.",
                        )
                    })?,
            )
        }
        _ => None,
    };
    if let Some(depth) = scan_depth {
        return Ok(LaunchRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            kind: "discovery".into(),
            path: Some(
                cwd.join(&args[2])
                    .to_str()
                    .ok_or_else(|| {
                        AppError::new(
                            "unsupported_path",
                            "Scan path must be representable as Unicode.",
                        )
                    })?
                    .into(),
            ),
            error: None,
            scan_depth: Some(depth),
        });
    }
    let (kind, path) = match args {
        [] => ("repository", cwd.to_path_buf()),
        [mode] if mode == "repo" => ("repository", cwd.to_path_buf()),
        [mode, path] if mode == "repo" => ("repository", cwd.join(path)),
        [mode, path] if mode == "group" && !path.starts_with('-') => ("project", cwd.join(path)),
        [path] if !path.starts_with('-') && path != "group" => ("repository", cwd.join(path)),
        _ => return Err(AppError::new("invalid_arguments", HELP)),
    };
    let path = path
        .to_str()
        .ok_or_else(|| {
            AppError::new(
                "unsupported_path",
                "Launcher path must be representable as Unicode.",
            )
        })?
        .to_owned();
    Ok(LaunchRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: kind.into(),
        path: Some(path),
        error: None,
        scan_depth: None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cwd_is_invocation_specific() {
        let cwd = std::env::temp_dir().join("launch space");
        let r = parse(&["group".into(), "work.json".into()], &cwd).unwrap();
        assert_eq!(r.path.unwrap(), cwd.join("work.json").to_str().unwrap());
        assert!(parse(&["group".into()], &cwd).is_err());
        assert!(parse(&["--unknown".into()], &cwd).is_err());
    }
    #[test]
    fn delivery_is_retained_until_correct_ack() {
        let q = LaunchQueue::default();
        let r = parse(&[], &std::env::temp_dir()).unwrap();
        q.push(r.clone());
        q.ack("wrong");
        assert_eq!(q.next().unwrap().request_id, r.request_id);
        q.ack(&r.request_id);
        assert!(q.next().is_none());
    }

    #[test]
    fn discovery_arguments_preserve_invoking_directory_and_validate_depth() {
        let cwd = std::env::temp_dir().join("invoking directory");
        let args = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let request = parse(&args(&["group", "--scan", "work"]), &cwd).unwrap();
        assert_eq!(request.kind, "discovery");
        assert_eq!(request.scan_depth, Some(3));
        assert_eq!(request.path.as_deref(), cwd.join("work").to_str());
        for depth in ["0", "10"] {
            assert_eq!(
                parse(&args(&["group", "--scan", "work", "--depth", depth]), &cwd)
                    .unwrap()
                    .scan_depth,
                Some(depth.parse().unwrap())
            );
        }
        for parts in [
            vec!["group", "--scan"],
            vec!["group", "--scan", "--depth"],
            vec!["group", "--scan", "work", "--depth"],
            vec!["group", "--scan", "work", "--depth", "11"],
            vec!["group", "--scan", "work", "--depth", "-1"],
            vec!["group", "--scan", "work", "--depth", "many"],
            vec!["group", "--scan", "work", "--other", "2"],
        ] {
            assert!(parse(&args(&parts), &cwd).is_err(), "{parts:?}");
        }
    }
}
