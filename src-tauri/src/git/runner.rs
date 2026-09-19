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
pub async fn run(
    cwd: &Path,
    args: &[OsString],
    input: Option<Vec<u8>>,
    mutation: bool,
    limit: usize,
) -> AppResult<Output> {
    let mut command = Command::new("git");
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
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(key);
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
    let output = tokio::time::timeout(Duration::from_secs(if mutation { 120 } else { 30 }), job)
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
        .map_err(|e| AppError::new("process_failed", e.to_string()))?;
    if !mutation && output.stdout.len() == limit && output.truncated {
        return Err(AppError::new(
            "output_limit",
            "Git output exceeded the structured data limit; no partial result was used.",
        ));
    }
    Ok(output)
}
pub fn checked(output: Output) -> AppResult<Output> {
    if output.code == 0 {
        Ok(output)
    } else {
        let mut error = AppError::new(
            "git_failed",
            format!("Git exited with status {}", output.code),
        );
        error.details = Some(redact(&String::from_utf8_lossy(&output.stderr)));
        Err(error)
    }
}
pub fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
pub async fn read(cwd: &Path, values: &[&str]) -> AppResult<Vec<u8>> {
    Ok(checked(run(cwd, &args(values), None, false, STRUCTURED_LIMIT).await?)?.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn structured_limit_is_an_error_not_partial_success() {
        let dir = tempfile::tempdir().unwrap();
        let error = run(dir.path(), &args(&["--version"]), None, false, 1)
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "output_limit");
        let output = run(dir.path(), &args(&["--version"]), None, true, 1)
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
}
