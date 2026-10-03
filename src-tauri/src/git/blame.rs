use super::*;
use std::collections::HashSet;

const BLOB_LIMIT: usize = 2 * 1024 * 1024;
const LINE_LIMIT: usize = 10_000;

#[derive(Clone)]
pub(crate) struct BlameSnapshot {
    repository_id: String,
    members: HashSet<String>,
    shallow_fingerprint: String,
}

struct Attribution {
    oid: String,
    original_line: usize,
    path: String,
    boundary: bool,
}

fn invalid() -> AppError {
    AppError::new(
        "invalid_git_output",
        "Incomplete or inconsistent blame output; no partial attribution was returned.",
    )
}

// Git C-quotes unusual filenames even with core.quotepath=false. These bytes
// are for display only; they never become a path argument or an identity token.
fn filename(value: &[u8]) -> AppResult<String> {
    if !value.starts_with(b"\"") {
        return Ok(text(value));
    }
    if !value.ends_with(b"\"") || value.len() < 2 {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    let mut chars = value[1..value.len() - 1].iter().copied();
    while let Some(byte) = chars.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let escaped = chars.next().ok_or_else(invalid)?;
        bytes.push(match escaped {
            b'a' => 7,
            b'b' => 8,
            b't' => 9,
            b'n' => 10,
            b'v' => 11,
            b'f' => 12,
            b'r' => 13,
            b'\\' => b'\\',
            b'"' => b'"',
            b'0'..=b'3' => {
                let second = chars
                    .next()
                    .filter(|b| (b'0'..=b'7').contains(b))
                    .ok_or_else(invalid)?;
                let third = chars
                    .next()
                    .filter(|b| (b'0'..=b'7').contains(b))
                    .ok_or_else(invalid)?;
                (escaped - b'0') * 64 + (second - b'0') * 8 + third - b'0'
            }
            _ => return Err(invalid()),
        });
    }
    Ok(text(&bytes))
}

fn parse(bytes: &[u8], contents: &[&str]) -> AppResult<Vec<Attribution>> {
    let mut records = bytes.split(|b| *b == b'\n');
    let mut result = Vec::new();
    for (index, content) in contents.iter().enumerate() {
        let header =
            std::str::from_utf8(records.next().ok_or_else(invalid)?).map_err(|_| invalid())?;
        let parts: Vec<_> = header.split(' ').collect();
        if !(3..=4).contains(&parts.len())
            || ![40, 64].contains(&parts[0].len())
            || !parts[0].bytes().all(|b| b.is_ascii_hexdigit())
            || parts[0].bytes().all(|b| b == b'0')
            || parts[2].parse::<usize>().ok() != Some(index + 1)
        {
            return Err(invalid());
        }
        let original_line = parts[1]
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(invalid)?;
        let mut path = None;
        let mut boundary = false;
        loop {
            let record = records.next().ok_or_else(invalid)?;
            if let Some(line) = record.strip_prefix(b"\t") {
                if line != content.as_bytes() {
                    return Err(invalid());
                }
                break;
            }
            if let Some(value) = record.strip_prefix(b"filename ") {
                path = Some(filename(value)?);
            }
            if record == b"boundary" {
                boundary = true;
            }
            if record == b"ignored" || record == b"unblamable" {
                return Err(invalid());
            }
        }
        result.push(Attribution {
            oid: parts[0].into(),
            original_line,
            path: path.ok_or_else(invalid)?,
            boundary,
        });
    }
    if records.any(|line| !line.is_empty()) {
        return Err(invalid());
    }
    Ok(result)
}

impl GitEngine {
    pub async fn get_blame(&self, request: BlameRequest) -> AppResult<BlameResult> {
        tokio::time::timeout(std::time::Duration::from_secs(30), self.blame_read(request))
            .await.map_err(|_| AppError::new("blame_timeout", "Blame exceeded 30 seconds. No partial attribution was returned; try an earlier revision or use Git."))?
    }

    async fn blame_read(&self, request: BlameRequest) -> AppResult<BlameResult> {
        let repo = self.repository(&request.repository_id).await?;
        let shallow = file_history::ancestry(&repo)?;
        let snapshot = self
            .snapshot(&request.repository_id, &request.snapshot_id)
            .await?;
        let file = snapshot
            .files
            .iter()
            .find(|file| file.wire.file_id == request.file_id)
            .ok_or_else(|| {
                AppError::new(
                    "stale_snapshot",
                    "File is outside the selected snapshot. Refresh and select it again.",
                )
            })?;
        let previous = request.side == FileHistoryPath::Previous;
        let path = if previous {
            file.old_path.as_ref().unwrap_or(&file.path)
        } else {
            &file.path
        };
        let working = snapshot.commit.is_none();
        let revision = if let Some((oid, parent)) = snapshot.commit {
            if previous {
                parent
            } else {
                Some(oid)
            }
        } else if let Some(status) = snapshot.status {
            if previous && file.old_path.is_none() {
                return Err(AppError::new(
                    "invalid_context",
                    "This working file has no recorded previous path.",
                ));
            }
            match status.head {
                Head::Branch { oid, .. } | Head::Detached { oid } => Some(oid),
                Head::Unborn { .. } => None,
            }
        } else {
            return Err(AppError::new(
                "invalid_context",
                "Select a working or historical file first.",
            ));
        };
        let mut result = BlameResult {
            snapshot_id: id(),
            revision_oid: revision.clone(),
            display_path: path.to_string_lossy().into_owned(),
            working,
            shallow: !shallow.is_empty(),
            unavailable_reason: None,
            lines: Vec::new(),
        };
        let Some(revision) = revision else {
            result.unavailable_reason = Some("There is no committed revision on this side (unborn HEAD or a root commit's empty parent).".into());
            return Ok(result);
        };
        let mut args = runner::args(&[
            "--no-replace-objects",
            "--literal-pathspecs",
            "ls-tree",
            "-l",
            "-z",
            "--full-tree",
            &revision,
            "--",
        ]);
        args.push(path.clone());
        let tree = runner::checked(
            self.runner
                .run(&repo.root, &args, None, false, runner::STRUCTURED_LIMIT)
                .await?,
        )?
        .stdout;
        let raw_path = os_bytes(path);
        let entry = tree.split(|b| *b == 0).find_map(|entry| {
            let tab = entry.iter().position(|b| *b == b'\t')?;
            (entry[tab + 1..] == raw_path).then_some(&entry[..tab])
        });
        let Some(entry) = entry else {
            result.unavailable_reason = Some("This path does not exist at this revision. For a deletion or rename, choose the comparison parent or previous HEAD path.".into());
            return Ok(result);
        };
        let header = text(entry);
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 4 {
            return Err(invalid());
        }
        if !["100644", "100755"].contains(&fields[0]) || fields[1] != "blob" {
            result.unavailable_reason = Some(
                "Blame supports regular files only, not symlinks, directories or submodules."
                    .into(),
            );
            return Ok(result);
        }
        if fields[3].parse::<usize>().map_err(|_| invalid())? > BLOB_LIMIT {
            result.unavailable_reason = Some(
                "Blame is limited to files of 2 MiB and 10,000 lines. Use Git for this file."
                    .into(),
            );
            return Ok(result);
        }
        let bytes = runner::checked(
            self.runner
                .run(
                    &repo.root,
                    &runner::args(&["--no-replace-objects", "cat-file", "blob", fields[2]]),
                    None,
                    false,
                    BLOB_LIMIT,
                )
                .await?,
        )?
        .stdout;
        let content = match std::str::from_utf8(&bytes) {
            Ok(content) if !bytes.contains(&0) => content,
            _ => {
                result.unavailable_reason = Some(
                    "Binary or non-UTF-8 content cannot be attributed in this text viewer.".into(),
                );
                return Ok(result);
            }
        };
        let lines: Vec<_> = if content.is_empty() {
            Vec::new()
        } else {
            content.split_terminator('\n').collect()
        };
        if lines.len() > LINE_LIMIT {
            result.unavailable_reason = Some(
                "Blame is limited to files of 2 MiB and 10,000 lines. Use Git for this file."
                    .into(),
            );
            return Ok(result);
        }
        let mut members = HashSet::new();
        if !lines.is_empty() {
            let mut args = runner::args(&[
                "--no-replace-objects",
                "--literal-pathspecs",
                "-c",
                "blame.blankBoundary=false",
                "-c",
                "diff.algorithm=myers",
                "blame",
                "--line-porcelain",
                "--root",
                "--encoding=UTF-8",
                "--no-textconv",
                "--no-ext-diff",
                "--ignore-revs-file=",
                &revision,
                "--",
            ]);
            args.push(path.clone());
            let output = runner::checked(
                self.runner
                    .run(&repo.root, &args, None, false, runner::STRUCTURED_LIMIT)
                    .await?,
            )?;
            let attribution = parse(&output.stdout, &lines)?;
            members.extend(attribution.iter().map(|line| line.oid.clone()));
            let mut oids: Vec<_> = members.iter().cloned().collect();
            oids.sort();
            let commits: HashMap<_, _> = history::commits(&self.runner, &repo, &oids)
                .await?
                .into_iter()
                .map(|c| (c.oid.clone(), c))
                .collect();
            for (index, attribution) in attribution.into_iter().enumerate() {
                let commit = commits.get(&attribution.oid).ok_or_else(invalid)?;
                result.lines.push(BlameLine {
                    line: index + 1,
                    original_line: attribution.original_line,
                    oid: attribution.oid,
                    author: commit.author.clone(),
                    subject: commit.message.lines().next().unwrap_or_default().into(),
                    original_path: attribution.path,
                    boundary: attribution.boundary
                        || shallow
                            .split(|b| *b == b'\n')
                            .any(|oid| oid == commit.oid.as_bytes()),
                    content: lines[index].into(),
                });
            }
        }
        if file_history::ancestry(&repo)? != shallow {
            return Err(AppError::new(
                "stale_snapshot",
                "Shallow ancestry changed during blame. Reopen it from the file.",
            ));
        }
        let mut state = self.state.lock().await;
        if state.blame.len() >= 32 {
            let old = state.blame.keys().next().cloned().unwrap();
            state.blame.remove(&old);
        }
        state.blame.insert(
            result.snapshot_id.clone(),
            BlameSnapshot {
                repository_id: request.repository_id,
                members,
                shallow_fingerprint: digest(&shallow),
            },
        );
        Ok(result)
    }

    pub async fn get_blame_commit(
        &self,
        repository_id: &str,
        blame_snapshot_id: &str,
        oid: &str,
        parent_oid: Option<String>,
    ) -> AppResult<CommitDetails> {
        let snapshot = self
            .state
            .lock()
            .await
            .blame
            .get(blame_snapshot_id)
            .filter(|s| s.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                AppError::new("stale_snapshot", "Blame expired. Reopen it from the file.")
            })?;
        if !snapshot.members.contains(oid) {
            return Err(AppError::new(
                "invalid_commit",
                "Select a commit attributed to a line in this blame result.",
            ));
        }
        let repo = self.repository(repository_id).await?;
        if digest(&file_history::ancestry(&repo)?) != snapshot.shallow_fingerprint {
            return Err(AppError::new(
                "stale_snapshot",
                "Shallow ancestry changed. Reopen blame.",
            ));
        }
        self.get_commit(repository_id, oid, parent_oid).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quoted_paths_and_incomplete_attribution() {
        assert_eq!(
            filename(br#""tab\tquote\"\n\316\262.txt""#).unwrap(),
            "tab\tquote\"\nβ.txt"
        );
        assert!(filename(br#""\777""#).is_err());
        assert!(parse(b"", &["line"]).is_err());
        let output = format!("{} 1 1 1\nfilename file\n\tline\n", "a".repeat(40));
        assert!(parse(output.as_bytes(), &["line"]).is_ok());
        assert!(parse(output.as_bytes(), &["different"]).is_err());
        assert!(parse(output.as_bytes(), &[]).is_err());
    }
}
