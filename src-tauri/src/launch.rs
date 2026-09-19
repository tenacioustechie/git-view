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
pub const HELP: &str = "Git View\n\nUsage:\n  git-view [path]\n  git-view repo [path]\n  git-view group <project.json>\n  git-view --help\n  git-view --version\n\nPaths resolve from the invoking directory. No launch action mutates Git.\n";
pub fn parse(args: &[String], cwd: &Path) -> AppResult<LaunchRequest> {
    let (kind, path) = match args {
        [] => ("repository", cwd.to_path_buf()),
        [mode] if mode == "repo" => ("repository", cwd.to_path_buf()),
        [mode, path] if mode == "repo" => ("repository", cwd.join(path)),
        [mode, path] if mode == "group" => ("project", cwd.join(path)),
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
}
