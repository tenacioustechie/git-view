use crate::contracts::{AppError, AppResult};
use std::{ffi::OsString, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

pub const DIAGNOSTIC_LIMIT: usize = 512 * 1024;
pub const STRUCTURED_LIMIT: usize = 16 * 1024 * 1024;
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: i32,
    pub truncated: bool,
}
async fn drain(mut pipe: impl AsyncRead + Unpin, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut chunk = [0; 8192];
    loop {
        let count = pipe.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let keep = count.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..keep]);
        truncated |= keep < count;
    }
    Ok((bytes, truncated))
}
pub fn redact(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            if word.contains("://") {
                "[remote URL]".to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
#[derive(Clone)]
pub(crate) struct Runner {
    pub isolated: bool,
    pub executable: OsString,
    pub read_timeout: Duration,
    pub mutation_timeout: Duration,
}
impl Default for Runner {
    fn default() -> Self {
        Self {
            isolated: false,
            executable: "git".into(),
            read_timeout: Duration::from_secs(30),
            mutation_timeout: Duration::from_secs(120),
        }
    }
}
impl Runner {
    pub async fn run(
        &self,
        cwd: &Path,
        args: &[OsString],
        input: Option<Vec<u8>>,
        mutation: bool,
        limit: usize,
    ) -> AppResult<Output> {
        let directory = std::fs::metadata(cwd).map_err(|error| {
            AppError::new(
                "repository_unavailable",
                format!("Cannot access repository directory: {error}"),
            )
        })?;
        if !directory.is_dir() {
            return Err(AppError::new(
                "repository_unavailable",
                "Repository path is not a directory.",
            ));
        }
        let mut command = Command::new(&self.executable);
        command
            .current_dir(cwd)
            .args([
                "--no-pager",
                "-c",
                "color.ui=false",
                "-c",
                "core.quotepath=false",
            ])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_CONFIG",
            "GIT_NAMESPACE",
            "GIT_SHALLOW_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        if self.isolated {
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            command
                .env("GIT_CONFIG_GLOBAL", null)
                .env("GIT_CONFIG_SYSTEM", null)
                .env("GIT_CONFIG_NOSYSTEM", "1");
            for key in [
                "GIT_TEMPLATE_DIR",
                "GIT_EXEC_PATH",
                "GIT_AUTHOR_NAME",
                "GIT_AUTHOR_EMAIL",
                "GIT_AUTHOR_DATE",
                "GIT_COMMITTER_NAME",
                "GIT_COMMITTER_EMAIL",
                "GIT_COMMITTER_DATE",
                "EMAIL",
                "GIT_SSH",
                "GIT_SSH_COMMAND",
                "GIT_ASKPASS",
                "SSH_ASKPASS",
            ] {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("GIT_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no");
        if !mutation {
            command.env("GIT_OPTIONAL_LOCKS", "0");
        } else {
            command.env_remove("GIT_OPTIONAL_LOCKS");
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().map_err(|e| {
            AppError::new(
                if e.kind() == std::io::ErrorKind::NotFound {
                    "git_unavailable"
                } else {
                    "process_failed"
                },
                e.to_string(),
            )
        })?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        // Machine output has its own explicit cap. Mutation diagnostics share one 512 KiB budget.
        let stdout_limit = if mutation {
            limit.min(DIAGNOSTIC_LIMIT / 2)
        } else {
            limit
        };
        let stderr_limit = if mutation {
            DIAGNOSTIC_LIMIT - stdout_limit
        } else {
            DIAGNOSTIC_LIMIT
        };
        let job = async {
            let writer = async {
                if let Some(bytes) = input {
                    stdin.write_all(&bytes).await?;
                }
                drop(stdin);
                Ok::<_, std::io::Error>(())
            };
            let (out, err, status, write) = tokio::join!(
                drain(stdout, stdout_limit),
                drain(stderr, stderr_limit),
                child.wait(),
                writer
            );
            let (stdout, truncated_out) = out?;
            let (stderr, truncated_err) = err?;
            let status = status?;
            if status.success() {
                write?;
            }
            Ok::<_, std::io::Error>(Output {
                stdout,
                stderr,
                code: status.code().unwrap_or(-1),
                truncated: truncated_out || truncated_err,
            })
        };
        let output = tokio::time::timeout(
            if mutation {
                self.mutation_timeout
            } else {
                self.read_timeout
            },
            job,
        )
        .await
        .map_err(|_| {
            AppError::new(
                if mutation {
                    "outcome_uncertain"
                } else {
                    "timeout"
                },
                "Git exceeded its time limit. Refresh repository state before retrying.",
            )
        })?
        .map_err(|e| {
            AppError::new(
                if mutation {
                    "outcome_uncertain"
                } else {
                    "process_failed"
                },
                e.to_string(),
            )
        })?;
        if mutation && output.code < 0 {
            let mut error=AppError::new("outcome_uncertain","Git was interrupted before a normal exit. Refresh repository state and review the operation before retrying.");
            error.output_truncated = Some(output.truncated);
            error.details = Some(redact(&String::from_utf8_lossy(&output.stderr)));
            return Err(error);
        }
        if !mutation && output.stdout.len() == limit && output.truncated {
            return Err(AppError::new(
                "output_limit",
                "Git output exceeded the structured data limit; no partial result was used.",
            ));
        }
        Ok(output)
    }
    pub async fn read(&self, cwd: &Path, values: &[&str]) -> AppResult<Vec<u8>> {
        Ok(checked(
            self.run(cwd, &args(values), None, false, STRUCTURED_LIMIT)
                .await?,
        )?
        .stdout)
    }
}

pub fn checked(output: Output) -> AppResult<Output> {
    if output.code == 0 {
        Ok(output)
    } else {
        let mut error = AppError::new(
            "git_failed",
            format!("Git exited with status {}", output.code),
        );
        error.exit_code = Some(output.code);
        error.output_truncated = Some(output.truncated);
        error.details = Some(redact(&String::from_utf8_lossy(&output.stderr)));
        Err(error)
    }
}
pub fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn structured_limit_is_an_error_not_partial_success() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner {
            isolated: true,
            ..Runner::default()
        };
        let error = runner
            .run(dir.path(), &args(&["--version"]), None, false, 1)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "output_limit");
        let output = runner
            .run(dir.path(), &args(&["--version"]), None, true, 1)
            .await
            .unwrap();
        assert!(output.truncated);
        assert_eq!(output.stdout.len(), 1);
    }
    #[test]
    fn credential_urls_are_not_retained() {
        assert_eq!(
            redact("failed https://user:secret@example.invalid/repo and ssh://user@example/repo"),
            "failed [remote URL] and [remote URL]"
        );
    }
    #[tokio::test]
    async fn timeouts_are_bounded_and_mutations_are_uncertain() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner {
            isolated: true,
            read_timeout: Duration::from_millis(100),
            mutation_timeout: Duration::from_millis(100),
            ..Runner::default()
        };
        for mutation in [false, true] {
            let start = std::time::Instant::now();
            let error = runner
                .run(
                    dir.path(),
                    &args(&["-c", "alias.wait=!sleep 2", "wait"]),
                    None,
                    mutation,
                    DIAGNOSTIC_LIMIT,
                )
                .await
                .err()
                .unwrap();
            assert_eq!(
                error.code,
                if mutation {
                    "outcome_uncertain"
                } else {
                    "timeout"
                }
            );
            assert!(start.elapsed() < Duration::from_secs(1));
        }
    }
    #[tokio::test]
    async fn diagnostic_streams_drain_concurrently_with_one_budget() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner {
            isolated: true,
            ..Runner::default()
        };
        let script="alias.noisy=!i=0; while test $i -lt 9000; do printf 0123456789012345678901234567890123456789012345678901234567890123; printf 0123456789012345678901234567890123456789012345678901234567890123 >&2; i=$((i+1)); done";
        let output = runner
            .run(
                dir.path(),
                &args(&["-c", script, "noisy"]),
                None,
                true,
                DIAGNOSTIC_LIMIT,
            )
            .await
            .unwrap();
        assert_eq!(output.code, 0);
        assert!(output.truncated);
        assert_eq!(output.stdout.len() + output.stderr.len(), DIAGNOSTIC_LIMIT);
    }
    #[tokio::test]
    async fn isolated_runner_ignores_system_and_global_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner {
            isolated: true,
            ..Runner::default()
        };
        let output = runner
            .read(dir.path(), &["config", "--show-scope", "--list"])
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&output);
        assert!(!text
            .lines()
            .any(|line| line.starts_with("global") || line.starts_with("system")));
        assert!(text.contains("command"));
    }
    #[tokio::test]
    async fn missing_git_is_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let runner = Runner {
            executable: dir.path().join("missing-git").into_os_string(),
            ..Runner::default()
        };
        assert_eq!(
            runner
                .read(dir.path(), &["--version"])
                .await
                .err()
                .unwrap()
                .code,
            "git_unavailable"
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn signaled_mutation_is_an_uncertain_outcome() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("interrupted-git");
        std::fs::write(&executable, "#!/bin/sh\nkill -TERM $$\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = Runner {
            executable: executable.into_os_string(),
            isolated: true,
            ..Runner::default()
        };
        assert_eq!(
            runner
                .run(dir.path(), &[], None, true, DIAGNOSTIC_LIMIT)
                .await
                .err()
                .unwrap()
                .code,
            "outcome_uncertain"
        );
    }
}
