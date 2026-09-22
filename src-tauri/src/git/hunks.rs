//! Complete text-hunk actions. Renderer identities never contain executable patch input.
use super::*;
use std::{
    fs,
    io::Read,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};
const LIMIT: usize = 2 * 1024 * 1024;
const MAX_HUNKS: usize = 1000;
const TRANSPORT_LIMIT: usize = 6 * 1024 * 1024;
fn unsupported(message: impl Into<String>) -> AppError {
    AppError::new("hunk_unavailable", message)
}
fn stale() -> AppError {
    AppError::new("precondition_changed","The file, index, attributes, or repository changed. Refresh its diff and review the hunk again.")
}
fn uncertain(message: impl Into<String>) -> AppError {
    AppError::new("outcome_uncertain", message)
}
#[derive(Clone)]
struct Hunk {
    id: String,
    range: Range<usize>,
    header: String,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}
#[derive(Clone)]
pub(crate) struct HunkSnapshot {
    repository_id: String,
    file_id: String,
    path: OsString,
    context: HunkContext,
    status_fingerprint: String,
    config: String,
    attributes: Vec<u8>,
    index_fingerprint: String,
    working_fingerprint: String,
    mode: String,
    index_oid: String,
    base: Arc<Vec<u8>>,
    diff: Arc<Vec<u8>>,
    hunks: Vec<Hunk>,
}
#[derive(Clone)]
pub(crate) struct HunkPlan {
    snapshot: Arc<HunkSnapshot>,
    patch: Arc<Vec<u8>>,
    expected_oid: String,
    expected_index: String,
}
fn regular_bytes(path: &Path) -> AppResult<Vec<u8>> {
    let meta = fs::symlink_metadata(path)
        .map_err(|e| unsupported(format!("Cannot inspect this working file: {e}")))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(unsupported(
            "Hunks require an ordinary regular file. Use the whole-file controls.",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(unsupported(
                "Hunks do not support reparse-point files. Use the whole-file controls.",
            ));
        }
    }
    if meta.len() > LIMIT as u64 {
        return Err(unsupported("Hunk actions are limited to files of 2 MiB."));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take((LIMIT + 1) as u64).read_to_end(&mut bytes))
        .map_err(|e| unsupported(e.to_string()))?;
    if bytes.len() > LIMIT {
        return Err(unsupported("Hunk actions are limited to files of 2 MiB."));
    }
    Ok(bytes)
}
fn selected_entry<'a>(entries: &'a [u8], path: &[u8]) -> AppResult<&'a [u8]> {
    let matches: Vec<_> = entries
        .split(|b| *b == 0)
        .filter(|r| r.split_once_byte(b'\t').is_some_and(|(_, p)| p == path))
        .collect();
    if matches.len() != 1 {
        return Err(unsupported(
            "Hunks require one resolved index entry. Use whole-file staging to resolve conflicts.",
        ));
    }
    Ok(matches[0])
}
trait ByteSplit {
    fn split_once_byte(&self, b: u8) -> Option<(&[u8], &[u8])>;
}
impl ByteSplit for [u8] {
    fn split_once_byte(&self, b: u8) -> Option<(&[u8], &[u8])> {
        self.iter()
            .position(|x| *x == b)
            .map(|i| (&self[..i], &self[i + 1..]))
    }
}
fn entry_parts(record: &[u8]) -> AppResult<(String, String)> {
    let (header, _) = record
        .split_once_byte(b'\t')
        .ok_or_else(|| unsupported("Malformed index record."))?;
    let fields: Vec<_> = header.split(|b| *b == b' ').collect();
    if fields.len() != 4
        || fields[0] != b"H"
        || fields[3] != b"0"
        || !matches!(fields[1], b"100644" | b"100755")
    {
        return Err(unsupported("Hunks require an ordinary resolved file without assume-unchanged or skip-worktree flags. Use whole-file controls."));
    }
    Ok((text(fields[1]), text(fields[2])))
}
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|b| *b == b'\n').collect()
}
fn range_header(value: &str, sign: char) -> AppResult<(u32, u32)> {
    let body = value
        .strip_prefix(sign)
        .ok_or_else(|| unsupported("Unsupported hunk header."))?;
    let (start, count) = body.split_once(',').unwrap_or((body, "1"));
    Ok((
        start
            .parse()
            .map_err(|_| unsupported("Invalid hunk line range."))?,
        count
            .parse()
            .map_err(|_| unsupported("Invalid hunk line count."))?,
    ))
}
fn parse_hunks(diff: &[u8]) -> AppResult<Vec<Hunk>> {
    let text=std::str::from_utf8(diff).map_err(|_|unsupported("Hunk actions require valid UTF-8 text; the preview contains other bytes. Use whole-file controls."))?;
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            starts.push(offset)
        } else if line.starts_with("diff --git ") && offset != 0 {
            return Err(unsupported("Hunks require a diff for exactly one path."));
        }
        offset += line.len();
    }
    if starts.is_empty() || starts.len() > MAX_HUNKS {
        return Err(unsupported(
            "Hunks require 1–1,000 complete text change blocks. Use whole-file controls.",
        ));
    }
    let mut hunks = Vec::new();
    for (i, start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(diff.len());
        let header = text[*start..end].lines().next().unwrap();
        let pieces: Vec<_> = header.splitn(5, ' ').collect();
        if pieces.len() < 4 || pieces[0] != "@@" || pieces[3] != "@@" {
            return Err(unsupported("Unsupported hunk header."));
        }
        let (old_start, old_lines) = range_header(pieces[1], '-')?;
        let (new_start, new_lines) = range_header(pieces[2], '+')?;
        hunks.push(Hunk {
            id: id(),
            range: *start..end,
            header: header.into(),
            old_start,
            old_lines,
            new_start,
            new_lines,
        });
    }
    Ok(hunks)
}
fn candidate(snapshot: &HunkSnapshot, hunk: &Hunk, build: bool) -> AppResult<Vec<u8>> {
    let block = &snapshot.diff[hunk.range.clone()];
    let first = block
        .iter()
        .position(|b| *b == b'\n')
        .ok_or_else(|| unsupported("Incomplete hunk header."))?;
    let mut old: Vec<Vec<u8>> = Vec::new();
    let mut new: Vec<Vec<u8>> = Vec::new();
    let mut previous = None;
    for line in lines(&block[first + 1..]) {
        if line == b"\\ No newline at end of file\n" || line == b"\\ No newline at end of file" {
            let strip = |v: &mut Vec<Vec<u8>>| -> AppResult<()> {
                if v.last_mut().and_then(Vec::pop) != Some(b'\n') {
                    return Err(unsupported("Malformed missing-newline marker."));
                }
                Ok(())
            };
            match previous {
                Some(b' ') => {
                    strip(&mut old)?;
                    strip(&mut new)?;
                }
                Some(b'-') => strip(&mut old)?,
                Some(b'+') => strip(&mut new)?,
                _ => return Err(unsupported("Unexpected missing-newline marker.")),
            }
            previous = None;
            continue;
        }
        match line.first() {
            Some(b' ') => {
                old.push(line[1..].to_vec());
                new.push(line[1..].to_vec());
            }
            Some(b'-') => old.push(line[1..].to_vec()),
            Some(b'+') => new.push(line[1..].to_vec()),
            _ => return Err(unsupported("Malformed text hunk body.")),
        }
        previous = line.first().copied();
    }
    if old.len() != hunk.old_lines as usize || new.len() != hunk.new_lines as usize {
        return Err(unsupported(
            "The hunk body does not match its complete line ranges.",
        ));
    }
    let (from, to, start, count) = match snapshot.context {
        HunkContext::Unstaged => (old, new, hunk.old_start, hunk.old_lines),
        HunkContext::Staged => (new, old, hunk.new_start, hunk.new_lines),
    };
    let start = if count == 0 {
        start as usize
    } else {
        start
            .checked_sub(1)
            .ok_or_else(|| unsupported("Invalid hunk start."))? as usize
    };
    let base_lines = lines(&snapshot.base);
    let end = start
        .checked_add(count as usize)
        .filter(|end| *end <= base_lines.len())
        .ok_or_else(|| unsupported("Hunk is outside the reviewed index blob."))?;
    if base_lines
        .get(start..end)
        .is_none_or(|actual| !actual.iter().copied().eq(from.iter().map(Vec::as_slice)))
    {
        return Err(unsupported("Hunk context does not match the exact reviewed index bytes. Refresh or use whole-file controls."));
    }
    if !build {
        return Ok(Vec::new());
    }
    let result: Vec<u8> = base_lines[..start]
        .iter()
        .copied()
        .chain(to.iter().map(Vec::as_slice))
        .chain(base_lines[end..].iter().copied())
        .flatten()
        .copied()
        .collect();
    if result.len() > LIMIT {
        return Err(unsupported(
            "The selected hunk would exceed the 2 MiB file limit.",
        ));
    }
    Ok(result)
}
fn quoted_path(prefix: &[u8], path: &[u8]) -> Vec<u8> {
    let mut result = vec![b'"'];
    for b in prefix.iter().chain(path) {
        match b {
            b'"' | b'\\' => {
                result.push(b'\\');
                result.push(*b)
            }
            0x20..=0x7e => result.push(*b),
            _ => result.extend_from_slice(format!("\\{b:03o}").as_bytes()),
        }
    }
    result.push(b'"');
    result
}
fn unselected_working(repo: &Repository, snapshot: &Snapshot, selected: &OsString) -> String {
    let mut bytes = Vec::new();
    for file in &snapshot.files {
        if file.path == *selected {
            continue;
        }
        bytes.extend_from_slice(&os_bytes(&file.path));
        bytes.push(0);
        bytes.extend_from_slice(
            format!(
                "{:?}:{:?}:{}:{}:{}",
                file.wire.index_status,
                file.wire.worktree_status,
                file.wire.untracked,
                file.wire.conflicted,
                file.wire.submodule
            )
            .as_bytes(),
        );
        if let Ok(meta) = fs::symlink_metadata(repo.root.join(&file.path)) {
            bytes.extend_from_slice(format!("{}:{:?}", meta.len(), meta.modified()).as_bytes());
        }
        bytes.push(0);
    }
    digest(&bytes)
}
impl GitEngine {
    async fn hunk_entries(&self, repo: &Repository) -> AppResult<Vec<u8>> {
        self.runner
            .read(&repo.root, &["ls-files", "-v", "--stage", "-z"])
            .await
    }
    async fn hunk_attributes(&self, repo: &Repository, path: &OsString) -> AppResult<Vec<u8>> {
        let mut args = runner::args(&[
            "--literal-pathspecs",
            "check-attr",
            "-z",
            "text",
            "eol",
            "filter",
            "ident",
            "working-tree-encoding",
            "diff",
            "--",
        ]);
        args.push(path.clone());
        Ok(runner::checked(
            self.runner
                .run(&repo.root, &args, None, false, runner::STRUCTURED_LIMIT)
                .await?,
        )?
        .stdout)
    }
    pub(crate) async fn hunk_basis(
        &self,
        repo: &Repository,
        snapshot: &Snapshot,
        file: &FileEntry,
        context: HunkContext,
    ) -> AppResult<HunkSnapshot> {
        let status = snapshot
            .status
            .as_ref()
            .ok_or_else(|| unsupported("Historical diffs are read-only."))?;
        if status.git_state != GitState::Idle || status.counts.conflicted > 0 {
            return Err(unsupported("Hunk actions require no ongoing operation or unresolved conflicts. Use whole-file conflict staging where available."));
        }
        if file.wire.untracked
            || file.wire.conflicted
            || file.wire.submodule
            || file.old_path.is_some()
            || file.wire.change_kind.as_deref() != Some("modified")
        {
            return Err(unsupported("Hunk actions support existing ordinary text modifications. Use whole-file controls for additions, deletions, renames, or special files."));
        }
        let raw_path = os_bytes(&file.path);
        if raw_path.is_empty()
            || Path::new(&file.path).is_absolute()
            || raw_path
                .split(|b| *b == b'/')
                .any(|part| part == b".." || part == b".")
        {
            return Err(unsupported("Unsupported hunk path."));
        }
        let config = self.config_fingerprint(repo).await?;
        let sparse = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["config", "--bool", "core.sparseCheckout"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if sparse.code == 0 && text(&sparse.stdout) == "true" {
            return Err(unsupported(
                "Hunk actions are not supported in sparse checkouts. Use whole-file controls.",
            ));
        }
        if sparse.code != 0 && sparse.code != 1 {
            runner::checked(sparse)?;
        }
        let attributes = self.hunk_attributes(repo, &file.path).await?;
        let records: Vec<_> = attributes
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect();
        if records.len() % 3 != 0 {
            return Err(unsupported("Malformed attribute information."));
        }
        for attr in records.chunks(3) {
            let allowed = match attr[1] {
                b"text" => matches!(attr[2], b"unspecified" | b"set" | b"unset" | b"auto"),
                b"eol" => matches!(attr[2], b"unspecified" | b"lf" | b"crlf"),
                b"diff" => attr[2] == b"unspecified",
                _ => matches!(attr[2], b"unspecified" | b"unset"),
            };
            if !allowed {
                return Err(unsupported(format!("Hunk actions do not support the effective {}={} attribute. Use whole-file controls.",text(attr[1]),text(attr[2]))));
            }
        }
        let entries = self.hunk_entries(repo).await?;
        let (mode, index_oid) = entry_parts(selected_entry(&entries, &raw_path)?)?;
        let mut head_args = runner::args(&[
            "--no-replace-objects",
            "--literal-pathspecs",
            "ls-tree",
            "-z",
            "HEAD",
            "--",
        ]);
        head_args.push(file.path.clone());
        let head = runner::checked(
            self.runner
                .run(
                    &repo.root,
                    &head_args,
                    None,
                    false,
                    runner::STRUCTURED_LIMIT,
                )
                .await?,
        )?
        .stdout;
        let (header,path)=head.strip_suffix(b"\0").and_then(|r|r.split_once_byte(b'\t')).ok_or_else(||unsupported("Hunks require a file already present in HEAD; use whole-file controls for additions."))?;
        if path != raw_path || header.split(|b| *b == b' ').next() != Some(mode.as_bytes()) {
            return Err(unsupported(
                "Hunks do not support file mode/type changes. Use whole-file controls.",
            ));
        }
        let working = regular_bytes(&repo.root.join(&file.path))?;
        if std::str::from_utf8(&working).is_err() || working.contains(&0) {
            return Err(unsupported(
                "Hunks require valid UTF-8 text without binary NUL bytes. Use whole-file controls.",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if self.config(repo, "core.filemode").await?.as_deref() != Some("false") {
                let executable = fs::metadata(repo.root.join(&file.path))
                    .map_err(|e| unsupported(e.to_string()))?
                    .permissions()
                    .mode()
                    & 0o111
                    != 0;
                if executable != (mode == "100755") {
                    return Err(unsupported(
                        "Hunks do not support executable-mode changes. Use whole-file controls.",
                    ));
                }
            }
        }
        let base = runner::checked(
            self.runner
                .run(
                    &repo.root,
                    &runner::args(&["--no-replace-objects", "cat-file", "blob", &index_oid]),
                    None,
                    false,
                    LIMIT,
                )
                .await?,
        )?
        .stdout;
        if std::str::from_utf8(&base).is_err() || base.contains(&0) {
            return Err(unsupported(
                "Hunks require a text index blob. Use whole-file controls.",
            ));
        }
        let result = HunkSnapshot {
            repository_id: repo.id.clone(),
            file_id: file.wire.file_id.clone(),
            path: file.path.clone(),
            context,
            status_fingerprint: snapshot.fingerprint.clone(),
            config,
            attributes,
            index_fingerprint: digest(&entries),
            working_fingerprint: digest(&working),
            mode,
            index_oid,
            base: Arc::new(base),
            diff: Arc::new(vec![]),
            hunks: vec![],
        };
        self.validate_hunk(repo, &result).await?;
        Ok(result)
    }
    async fn validate_hunk(&self, repo: &Repository, snapshot: &HunkSnapshot) -> AppResult<()> {
        if self.observe(repo).await?.fingerprint != snapshot.status_fingerprint
            || self.config_fingerprint(repo).await? != snapshot.config
            || self.hunk_attributes(repo, &snapshot.path).await? != snapshot.attributes
            || digest(&self.hunk_entries(repo).await?) != snapshot.index_fingerprint
            || digest(&regular_bytes(&repo.root.join(&snapshot.path))?)
                != snapshot.working_fingerprint
        {
            return Err(stale());
        }
        Ok(())
    }
    pub(crate) async fn issue_hunks(
        &self,
        repo: &Repository,
        mut snapshot: HunkSnapshot,
        diff: Vec<u8>,
    ) -> AppResult<HunkActions> {
        if diff.len() > LIMIT {
            return Err(unsupported("Hunk preview exceeds 2 MiB."));
        }
        let hunks = parse_hunks(&diff)?;
        let preamble = String::from_utf8(diff[..hunks[0].range.start].to_vec())
            .map_err(|_| unsupported("Invalid UTF-8 preview."))?;
        if preamble.lines().any(|l| {
            l.starts_with("new file mode ")
                || l.starts_with("deleted file mode ")
                || l.starts_with("old mode ")
                || l.starts_with("new mode ")
                || l.starts_with("rename ")
                || l.starts_with("copy ")
        }) {
            return Err(unsupported(
                "Use whole-file controls for file identity or mode changes.",
            ));
        }
        snapshot.diff = Arc::new(diff);
        snapshot.hunks = hunks;
        for hunk in &snapshot.hunks {
            candidate(&snapshot, hunk, false)?;
        }
        self.validate_hunk(repo, &snapshot).await?;
        let wire = snapshot
            .hunks
            .iter()
            .map(|h| DiffHunk {
                hunk_id: h.id.clone(),
                header: h.header.clone(),
                old_start: h.old_start,
                old_lines: h.old_lines,
                new_start: h.new_start,
                new_lines: h.new_lines,
                text: String::from_utf8(snapshot.diff[h.range.clone()].to_vec())
                    .expect("validated UTF-8"),
            })
            .collect();
        let diff_snapshot_id = id();
        let context = snapshot.context;
        let mut state = self.state.lock().await;
        if state.hunks.len() == 16 {
            state.hunks.pop_front();
        }
        state
            .hunks
            .push_back((diff_snapshot_id.clone(), Arc::new(snapshot)));
        Ok(HunkActions::Available {
            diff_snapshot_id,
            context,
            preamble,
            hunks: wire,
        })
    }
}
impl GitEngine {
    async fn hunk_transport(
        &self,
        snapshot: &HunkSnapshot,
        candidate: &[u8],
    ) -> AppResult<(Vec<u8>, String)> {
        let directory = tempfile::tempdir().map_err(|e| unsupported(e.to_string()))?;
        let root = directory.path();
        let mut encoder = self.runner.clone();
        encoder.isolated = true;
        encoder.locale = Some("C".into());
        let format = match snapshot.index_oid.len() {
            40 => "--object-format=sha1",
            64 => "--object-format=sha256",
            _ => return Err(unsupported("Unsupported repository object format.")),
        };
        runner::checked(
            encoder
                .run(
                    root,
                    &runner::args(&["init", "--quiet", "--template=", format]),
                    None,
                    false,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await?,
        )?;
        fs::create_dir_all(root.join(".git/info")).map_err(|e| unsupported(e.to_string()))?;
        fs::write(root.join(".git/info/attributes"), b"* -diff\n")
            .map_err(|e| unsupported(e.to_string()))?;
        fs::write(root.join("before"), snapshot.base.as_slice())
            .map_err(|e| unsupported(e.to_string()))?;
        fs::write(root.join("after"), candidate).map_err(|e| unsupported(e.to_string()))?;
        let output = encoder
            .run(
                root,
                &runner::args(&[
                    "-c",
                    "core.attributesFile=",
                    "-c",
                    "core.hooksPath=.disabled-hooks",
                    "diff",
                    "--no-index",
                    "--binary",
                    "--full-index",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    "--src-prefix=a/",
                    "--dst-prefix=b/",
                    "before",
                    "after",
                ]),
                None,
                false,
                TRANSPORT_LIMIT,
            )
            .await?;
        if output.code != 1 {
            runner::checked(output)?;
            return Err(unsupported("The selected hunk produces no index change."));
        }
        let bytes = output.stdout;
        let parts = lines(&bytes);
        if parts.len() < 3
            || parts[0] != b"diff --git a/before b/after\n"
            || parts[2] != b"GIT binary patch\n"
        {
            return Err(unsupported(
                "Git did not produce the required exact-blob transport patch.",
            ));
        }
        let header = std::str::from_utf8(parts[1])
            .map_err(|_| unsupported("Invalid binary transport header."))?;
        let pair = header
            .strip_prefix("index ")
            .and_then(|s| s.split_once(' '))
            .map(|(s, _)| s)
            .ok_or_else(|| unsupported("Invalid binary transport object IDs."))?;
        let (old, new) = pair
            .split_once("..")
            .ok_or_else(|| unsupported("Invalid binary transport object IDs."))?;
        if old != snapshot.index_oid
            || new.len() != old.len()
            || !new.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(unsupported(
                "Transport preimage does not match the exact reviewed index blob.",
            ));
        }
        let path = os_bytes(&snapshot.path);
        let mut patch = b"diff --git ".to_vec();
        patch.extend(quoted_path(b"a/", &path));
        patch.push(b' ');
        patch.extend(quoted_path(b"b/", &path));
        patch.push(b'\n');
        patch.extend(format!("index {old}..{new} {}\n", snapshot.mode).as_bytes());
        patch.extend_from_slice(&bytes[parts[0].len() + parts[1].len()..]);
        if patch.len() > TRANSPORT_LIMIT {
            return Err(unsupported(
                "Internal hunk transport exceeds its 6 MiB bound.",
            ));
        }
        Ok((patch, new.into()))
    }
    pub(crate) async fn prepare_hunk(
        &self,
        repo: &Repository,
        action: &RepositoryAction,
    ) -> AppResult<(HunkPlan, String)> {
        let (diff_snapshot_id, file_id, hunk_id, context) = match action {
            RepositoryAction::StageHunk {
                diff_snapshot_id,
                file_id,
                hunk_id,
            } => (diff_snapshot_id, file_id, hunk_id, HunkContext::Unstaged),
            RepositoryAction::UnstageHunk {
                diff_snapshot_id,
                file_id,
                hunk_id,
            } => (diff_snapshot_id, file_id, hunk_id, HunkContext::Staged),
            _ => return Err(unsupported("Invalid hunk action.")),
        };
        let snapshot = self
            .state
            .lock()
            .await
            .hunks
            .iter()
            .find(|(id, _)| id == diff_snapshot_id)
            .map(|(_, s)| s.clone())
            .ok_or_else(stale)?;
        if snapshot.repository_id != repo.id
            || snapshot.file_id != *file_id
            || snapshot.context != context
        {
            return Err(stale());
        }
        let hunk = snapshot
            .hunks
            .iter()
            .find(|h| h.id == *hunk_id)
            .ok_or_else(stale)?;
        self.validate_hunk(repo, &snapshot).await?;
        let candidate = candidate(&snapshot, hunk, true)?;
        let (patch, expected_oid) = self.hunk_transport(&snapshot, &candidate).await?;
        let entries = self.hunk_entries(repo).await?;
        if digest(&entries) != snapshot.index_fingerprint {
            return Err(stale());
        }
        let path = os_bytes(&snapshot.path);
        let selected = selected_entry(&entries, &path)?;
        let mut expected = Vec::with_capacity(entries.len());
        for entry in entries.split_inclusive(|b| *b == 0) {
            if entry.strip_suffix(b"\0") == Some(selected) {
                expected.extend(format!("H {} {} 0\t", snapshot.mode, expected_oid).as_bytes());
                expected.extend_from_slice(&path);
                expected.push(0);
            } else {
                expected.extend_from_slice(entry)
            }
        }
        self.validate_hunk(repo, &snapshot).await?;
        let description=format!("{} the complete hunk {} in {}. Change only this file's index content; keep its working bytes. Other hunks stay as reviewed. Normal Git index hooks run; unexpected hook or external changes require refresh.",if context==HunkContext::Unstaged{"Stage"}else{"Unstage"},hunk.header,snapshot.path.to_string_lossy());
        Ok((
            HunkPlan {
                snapshot,
                patch: Arc::new(patch),
                expected_oid,
                expected_index: digest(&expected),
            },
            description,
        ))
    }
    pub(crate) async fn execute_hunk(
        &self,
        plan: &PreparedAction,
        hunk: &HunkPlan,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        let snapshot = &hunk.snapshot;
        self.validate_hunk(repo, snapshot).await?;
        let before = self.observe(repo).await?;
        let working_before = unselected_working(repo, &before, &snapshot.path);
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Canceled before the index hunk update.",
            ));
        }
        let args = runner::args(&[
            "--no-replace-objects",
            "-c",
            "apply.ignoreWhitespace=false",
            "apply",
            "--cached",
            "--whitespace=nowarn",
            "-",
        ]);
        let outcome = self
            .runner
            .run(
                &repo.root,
                &args,
                Some(hunk.patch.as_ref().clone()),
                true,
                runner::DIAGNOSTIC_LIMIT,
            )
            .await;
        let post=async {
   let observed=self.observe(repo).await?;let entries=self.hunk_entries(repo).await?;
   let exact=digest(&entries)==hunk.expected_index && digest(&regular_bytes(&repo.root.join(&snapshot.path))?)==snapshot.working_fingerprint && self.config_fingerprint(repo).await?==snapshot.config && self.hunk_attributes(repo,&snapshot.path).await?==snapshot.attributes && observed.status.as_ref().unwrap().head==before.status.as_ref().unwrap().head && observed.status.as_ref().unwrap().git_state==GitState::Idle && unselected_working(repo,&observed,&snapshot.path)==working_before;
   Ok::<_,AppError>((exact,entries))
  }.await.map_err(|e|uncertain(format!("Git attempted the hunk update, but its resulting state could not be verified: {}. Refresh and inspect; no rollback was attempted.",e.message)))?;
        match outcome.and_then(runner::checked) {
   Ok(output) if post.0=>Ok(ActionOutcome{exit_code:output.code,output_truncated:output.truncated}),
   Ok(_)=>Err(uncertain("Git updated the index, but hooks or external activity changed the expected index, working files, branch, attributes, or configuration. Refresh and inspect; no rollback was attempted.")),
   Err(mut error)=>{let actual=selected_entry(&post.1,&os_bytes(&snapshot.path)).ok().and_then(|r|entry_parts(r).ok()).map(|(_,oid)|oid);if actual.as_deref()==Some(&hunk.expected_oid) {error.code="outcome_uncertain".into();error.message=format!("{} The selected hunk appears in the index despite the reported failure. Refresh and inspect before retrying.",error.message);}Err(error)}
  }
    }
}
