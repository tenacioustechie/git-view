use super::*;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
};

type DiagnosticDrain = tokio::task::JoinHandle<std::io::Result<(Vec<u8>, bool)>>;

pub(crate) struct HeadGuard {
    timeout: Duration,
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    stderr: Option<DiagnosticDrain>,
}
impl HeadGuard {
    pub async fn acquire(
        runner: &runner::Runner,
        repo: &Repository,
        branch: &str,
    ) -> AppResult<Self> {
        Self::prepare(
            runner,
            &repo.root,
            &[],
            format!("option no-deref\nsymref-verify HEAD {branch}\n"),
        )
        .await
    }
    pub async fn prepare(
        runner: &runner::Runner,
        path: &Path,
        prefix: &[OsString],
        body: String,
    ) -> AppResult<Self> {
        let mut args = prefix.to_vec();
        args.extend(runner::args(&["update-ref", "--stdin"]));
        let mut command = runner.command(path, &args, true);
        command.kill_on_drop(false);
        let mut child = command
            .spawn()
            .map_err(|error| AppError::new("rewrite_guard_unavailable", error.to_string()))?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let stderr = child.stderr.take().unwrap();
        let mut guard = Self {
            timeout: runner.mutation_timeout,
            child: Some(child),
            input: Some(input),
            output,
            stderr: Some(tokio::spawn(runner::drain(
                stderr,
                runner::DIAGNOSTIC_LIMIT,
            ))),
        };
        let input = format!("start\n{body}prepare\n");
        guard
            .input
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .await
            .map_err(|error| AppError::new("rewrite_guard_unavailable", error.to_string()))?;
        guard.ack("start: ok").await?;
        guard.ack("prepare: ok").await?;
        Ok(guard)
    }
    async fn ack(&mut self, expected: &str) -> AppResult<()> {
        let result = tokio::time::timeout(self.timeout, async {
            let mut bytes = Vec::new();
            loop {
                let byte = self.output.read_u8().await?;
                if byte == b'\n' {
                    break;
                }
                if bytes.len() == 1024 {
                    return Err(std::io::Error::other("Reference transaction output limit"));
                }
                bytes.push(byte);
            }
            Ok::<_, std::io::Error>(
                String::from_utf8_lossy(&bytes)
                    .trim_end_matches('\r')
                    .to_owned(),
            )
        })
        .await;
        match result {Ok(Ok(line)) if line==expected=>Ok(()),_=>Err(AppError::new("rewrite_guard_unavailable","Git could not prepare its symbolic HEAD transaction. A supporting Git version and successful reference hooks are required; no original branch update was authorized."))}
    }
    pub async fn release(self) -> AppResult<()> {
        self.finish("abort").await
    }
    pub async fn commit(self) -> AppResult<()> {
        self.finish("commit").await
    }
    async fn finish(mut self, verb: &str) -> AppResult<()> {
        self.input
            .as_mut()
            .unwrap()
            .write_all(format!("{verb}\n").as_bytes())
            .await
            .map_err(|error| AppError::new("outcome_uncertain", error.to_string()))?;
        self.ack(&format!("{verb}: ok")).await.map_err(|_| {
            AppError::new(
                "outcome_uncertain",
                "HEAD guard release was not acknowledged. Reconcile actual refs before retrying.",
            )
        })?;
        self.input.take();
        let status = tokio::time::timeout(
            self.timeout.min(Duration::from_secs(10)),
            self.child.as_mut().unwrap().wait(),
        )
        .await
        .map_err(|_| {
            AppError::new(
                "outcome_uncertain",
                "HEAD guard did not release within its time limit.",
            )
        })?
        .map_err(|error| AppError::new("outcome_uncertain", error.to_string()))?;
        if !status.success() {
            return Err(AppError::new(
                "outcome_uncertain",
                "HEAD guard ended unexpectedly; refresh actual refs.",
            ));
        }
        if let Some(stderr) = self.stderr.take() {
            stderr.abort();
        }
        Ok(())
    }
}
impl Drop for HeadGuard {
    fn drop(&mut self) {
        // EOF makes Git abort its own prepared transaction. Never create or delete
        // Git lockfiles ourselves. Only force process termination if EOF cannot finish.
        self.input.take();
        if let Some(stderr) = self.stderr.take() {
            stderr.abort();
        }
        if let Some(mut child) = self.child.take() {
            let timeout = self.timeout.min(Duration::from_secs(10));
            tokio::spawn(async move {
                if tokio::time::timeout(timeout, child.wait()).await.is_err() {
                    let _ = child.kill().await;
                }
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn unacknowledged_release_terminates_and_reaps_guard_child() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-git");
        std::fs::write(&executable,"#!/bin/sh\necho $$ > guard-pid\nprintf 'start: ok\\nprepare: ok\\n'\nwhile :; do read -r ignored || :; done\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = runner::Runner {
            executable: executable.into_os_string(),
            mutation_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let mut guard = HeadGuard::prepare(&runner, dir.path(), &[], String::new())
            .await
            .unwrap();
        guard.timeout = Duration::from_millis(50);
        let pid = std::fs::read_to_string(dir.path().join("guard-pid")).unwrap();
        assert_eq!(
            guard.release().await.err().unwrap().code,
            "outcome_uncertain"
        );
        let mut alive = true;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            alive = std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .output()
                .unwrap()
                .status
                .success();
            if !alive {
                break;
            }
        }
        assert!(
            !alive,
            "Guard subprocess was orphaned after release timeout"
        );
    }
}
