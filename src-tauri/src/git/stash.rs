use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
};
const LIMIT: usize = 24 * 1024 * 1024;
const LOG_LIMIT: usize = 8 * 1024 * 1024;
mod encoded {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let value = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(serde::de::Error::custom)
    }
}
const MAX_ENTRIES: usize = 1000;
fn error(message: impl Into<String>) -> AppError {
    AppError::new("stash_unavailable", message)
}
fn stale() -> AppError {
    AppError::new(
        "precondition_changed",
        "The stash list changed. Refresh it and review the selected entry again.",
    )
}
fn io(error: std::io::Error) -> AppError {
    AppError::new("stash_io", error.to_string())
}
fn uncertain(message: impl Into<String>) -> AppError {
    AppError::new("outcome_uncertain", message)
}
fn valid_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    tip: Option<String>,
    log: Vec<u8>,
    stamp: String,
}
#[derive(Clone)]
pub(crate) struct Row {
    oid: String,
    tail: Vec<u8>,
    entry: StashEntry,
}
#[derive(Clone)]
pub(crate) struct StashSnapshot {
    common_dir: PathBuf,
    pair: Pair,
    rows: Vec<Row>,
}
#[derive(Clone)]
pub(crate) enum StashPlan {
    Create,
    Apply {
        snapshot: StashSnapshot,
        row: Row,
    },
    Delete {
        snapshot: StashSnapshot,
        index: usize,
    },
    Finalize {
        journal: Journal,
        bytes: Vec<u8>,
    },
}
#[derive(Clone)]
struct Shape {
    base: String,
    index: String,
    untracked: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Journal {
    version: u8,
    recovery_id: String,
    common_dir: PathBuf,
    old_tip: String,
    new_tip: Option<String>,
    #[serde(with = "encoded")]
    old_log: Vec<u8>,
    #[serde(with = "encoded")]
    new_log: Vec<u8>,
    selected_oid: String,
    selected_message: String,
    backup_ref: String,
}
fn log_path(repo: &Repository) -> PathBuf {
    repo.common_dir.join("logs/refs/stash")
}
fn journal_path(repo: &Repository) -> PathBuf {
    repo.common_dir.join("git-view-stash-delete.json")
}
fn regular_read(path: &Path) -> AppResult<Vec<u8>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(io(e)),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > LIMIT as u64 {
        return Err(error(
            "Stash metadata is not a bounded regular file; inspect it outside Git View.",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(error("Stash metadata uses an unsupported reparse point."));
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > LIMIT {
        return Err(error("Stash metadata exceeds the read limit."));
    }
    Ok(bytes)
}
fn stamp(path: &Path) -> AppResult<String> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                Ok(format!(
                    "{}:{}:{}:{}:{}:{}",
                    m.dev(),
                    m.ino(),
                    m.len(),
                    m.mtime(),
                    m.mtime_nsec(),
                    m.ctime_nsec()
                ))
            }
            #[cfg(not(unix))]
            {
                Ok(format!("{}:{:?}:{:?}", m.len(), m.modified(), m.created()))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("missing".into()),
        Err(e) => Err(io(e)),
    }
}
fn rows(bytes: &[u8]) -> AppResult<Vec<Row>> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(error("Stash reflog has an incomplete record."));
    }
    let mut result = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        if result.len() == MAX_ENTRIES {
            return Err(error(
                "Stash list exceeds 1,000 entries. Manage or archive entries with Git first.",
            ));
        }
        let first = line
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(|| error("Malformed stash reflog."))?;
        let next = line[first + 1..]
            .iter()
            .position(|b| *b == b' ')
            .map(|n| n + first + 1)
            .ok_or_else(|| error("Malformed stash reflog."))?;
        let oid = text(&line[first + 1..next]);
        if !valid_oid(&oid) || !valid_oid(&text(&line[..first])) {
            return Err(error("Malformed stash reflog object ID."));
        }
        let tail = line[next + 1..].to_vec();
        let tab = tail
            .iter()
            .position(|b| *b == b'\t')
            .ok_or_else(|| error("Malformed stash reflog metadata."))?;
        let identity = text(&tail[..tab]);
        let mut fields = identity.rsplitn(3, ' ');
        let zone = fields.next().unwrap_or_default();
        let seconds = fields.next().unwrap_or_default();
        let date = seconds
            .parse::<i64>()
            .ok()
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| format!("{seconds} {zone}"));
        let message = String::from_utf8_lossy(&tail[tab + 1..]).into_owned();
        result.push(Row {
            oid: oid.clone(),
            tail,
            entry: StashEntry {
                entry_id: id(),
                oid,
                message,
                created_at: date,
                branch: None,
                has_untracked: false,
                unavailable_reason: None,
            },
        });
    }
    Ok(result)
}
impl GitEngine {
    async fn stash_tip(&self, repo: &Repository) -> AppResult<Option<String>> {
        let sym = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["symbolic-ref", "-q", "refs/stash"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if sym.code == 0 {
            return Err(error(
                "refs/stash is symbolic; inspect it outside Git View.",
            ));
        }
        if sym.code != 1 {
            runner::checked(sym)?;
        }
        let output = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["rev-parse", "--verify", "--quiet", "refs/stash"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if output.code == 1 {
            return Ok(None);
        }
        let oid = text(&runner::checked(output)?.stdout);
        if !valid_oid(&oid) {
            return Err(error("Invalid stash ref."));
        }
        Ok(Some(oid))
    }
    async fn stash_pair(&self, repo: &Repository) -> AppResult<Pair> {
        let format = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["config", "--local", "--get", "extensions.refStorage"]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        let backend = if format.code == 1 {
            "files".into()
        } else {
            text(&runner::checked(format)?.stdout)
        };
        if backend != "files" {
            return Err(error("Stash inspection currently requires Git's files reference backend; this repository uses another backend."));
        }
        let tip = self.stash_tip(repo).await?;
        let path = log_path(repo);
        let before = stamp(&path)?;
        let log = regular_read(&path)?;
        if log.len() > LOG_LIMIT {
            return Err(error("Stash reflog exceeds the 8 MiB inspection limit."));
        }
        let after = stamp(&path)?;
        if before != after || tip != self.stash_tip(repo).await? {
            return Err(stale());
        }
        Ok(Pair {
            tip,
            log,
            stamp: after,
        })
    }
    async fn stash_shape(&self, repo: &Repository, oid: &str) -> AppResult<Shape> {
        let raw = self
            .runner
            .read(
                &repo.root,
                &["--no-replace-objects", "cat-file", "commit", oid],
            )
            .await?;
        let headers = raw.split(|b| *b == b'\n').take_while(|l| !l.is_empty());
        let parents: Vec<_> = headers
            .filter_map(|l| l.strip_prefix(b"parent ").map(text))
            .collect();
        if !(2..=3).contains(&parents.len()) {
            return Err(error(
                "This entry is not a supported two- or three-parent stash commit.",
            ));
        }
        let index = self
            .runner
            .read(
                &repo.root,
                &["--no-replace-objects", "cat-file", "commit", &parents[1]],
            )
            .await?;
        let ip: Vec<_> = index
            .split(|b| *b == b'\n')
            .take_while(|l| !l.is_empty())
            .filter_map(|l| l.strip_prefix(b"parent ").map(text))
            .collect();
        if ip != vec![parents[0].clone()] {
            return Err(error("Stash index parent does not match its base."));
        }
        if let Some(u) = parents.get(2) {
            let raw = self
                .runner
                .read(
                    &repo.root,
                    &["--no-replace-objects", "cat-file", "commit", u],
                )
                .await?;
            if raw
                .split(|b| *b == b'\n')
                .take_while(|l| !l.is_empty())
                .any(|l| l.starts_with(b"parent "))
            {
                return Err(error("Stash untracked snapshot is not a root commit."));
            }
        }
        Ok(Shape {
            base: parents[0].clone(),
            index: parents[1].clone(),
            untracked: parents.get(2).cloned(),
        })
    }
    pub async fn list_stashes(&self, repository_id: &str) -> AppResult<StashList> {
        let repo = self.repository(repository_id).await?;
        let pair = self.stash_pair(&repo).await?;
        let recovery = self.stash_recovery(&repo).await?;
        let mut entries = rows(&pair.log)?;
        if entries.last().map(|r| &r.oid) != pair.tip.as_ref() && recovery.is_none() {
            return Err(error(
                "The stash ref and reflog disagree. Inspect them before using stash actions.",
            ));
        }
        // Keep a selected occurrence stable across an unchanged list refresh.
        // The entire exact pair (including file identity/time evidence) belongs
        // to this token: repeated identical rows remain distinct by ordinal,
        // and every token changes when another writer changes the list.
        let list_identity = digest(
            format!(
                "{}\0{}\0{}\0{}",
                digest(&os_bytes(repo.common_dir.as_os_str())),
                pair.tip.as_deref().unwrap_or_default(),
                digest(&pair.log),
                pair.stamp
            )
            .as_bytes(),
        );
        for (ordinal, row) in entries.iter_mut().enumerate() {
            row.entry.entry_id = digest(format!("{list_identity}\0{ordinal}").as_bytes());
            match self.stash_shape(&repo, &row.oid).await {
                Ok(shape) => row.entry.has_untracked = shape.untracked.is_some(),
                Err(e) => row.entry.unavailable_reason = Some(e.message),
            }
        }
        if pair != self.stash_pair(&repo).await? {
            return Err(stale());
        }
        let snapshot_id = id();
        let wire = entries.iter().rev().map(|r| r.entry.clone()).collect();
        let mut state = self.state.lock().await;
        if state.stashes.len() >= 64 {
            state.stashes.clear();
        }
        state.stashes.insert(
            snapshot_id.clone(),
            StashSnapshot {
                common_dir: repo.common_dir.clone(),
                pair,
                rows: entries,
            },
        );
        Ok(StashList {
            snapshot_id,
            entries: wire,
            delete_unavailable_reason: (!stash_atomic::supported())
                .then(|| "Safe stash deletion is not implemented on this operating system.".into()),
            recovery,
        })
    }
    async fn stash_selection(
        &self,
        repo: &Repository,
        snapshot_id: &str,
        entry_id: &str,
    ) -> AppResult<(StashSnapshot, usize)> {
        let snapshot = self
            .state
            .lock()
            .await
            .stashes
            .get(snapshot_id)
            .cloned()
            .ok_or_else(stale)?;
        if snapshot.common_dir != repo.common_dir || snapshot.pair != self.stash_pair(repo).await? {
            return Err(stale());
        }
        let index = snapshot
            .rows
            .iter()
            .position(|r| r.entry.entry_id == entry_id)
            .ok_or_else(stale)?;
        Ok((snapshot, index))
    }
    pub async fn get_stash_details(
        &self,
        repository_id: &str,
        list_snapshot_id: &str,
        entry_id: &str,
        section: StashSection,
    ) -> AppResult<CommitDetails> {
        let repo = self.repository(repository_id).await?;
        let (snapshot, index) = self
            .stash_selection(&repo, list_snapshot_id, entry_id)
            .await?;
        let oid = &snapshot.rows[index].oid;
        let shape = self.stash_shape(&repo, oid).await?;
        let result = match section {
            StashSection::Worktree => self.get_commit(repository_id, oid, Some(shape.base)).await,
            StashSection::Index => {
                self.get_commit(repository_id, &shape.index, Some(shape.base))
                    .await
            }
            StashSection::Untracked => {
                self.get_commit(
                    repository_id,
                    &shape
                        .untracked
                        .ok_or_else(|| error("This stash has no saved untracked files."))?,
                    None,
                )
                .await
            }
        }?;
        if snapshot.pair != self.stash_pair(&repo).await? {
            return Err(stale());
        }
        Ok(result)
    }
}
fn sync_directory(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(io)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    sync_directory(
        path.parent()
            .ok_or_else(|| error("Missing metadata parent."))?,
    )
}
fn filtered(rows: &[Row], skip: usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut previous = "0".repeat(rows[skip].oid.len());
    for (i, row) in rows.iter().enumerate() {
        if i == skip {
            continue;
        }
        result.extend_from_slice(format!("{previous} {} ", row.oid).as_bytes());
        result.extend_from_slice(&row.tail);
        result.push(b'\n');
        previous = row.oid.clone();
    }
    result
}
fn read_journal(repo: &Repository) -> AppResult<Option<(Journal, Vec<u8>)>> {
    let bytes = regular_read(&journal_path(repo))?;
    if bytes.is_empty() {
        if journal_path(repo).exists() {
            return Err(error("Empty stash Delete journal requires manual review."));
        }
        return Ok(None);
    }
    let j: Journal = serde_json::from_slice(&bytes)
        .map_err(|_| error("Corrupt stash Delete journal; preserve it and inspect manually."))?;
    if j.version != 1
        || uuid::Uuid::parse_str(&j.recovery_id).is_err()
        || j.common_dir != repo.common_dir
        || !valid_oid(&j.old_tip)
        || !valid_oid(&j.selected_oid)
        || j.new_tip.as_ref().is_some_and(|s| !valid_oid(s))
        || j.backup_ref != format!("refs/git-view/stash-deletions/{}", j.recovery_id)
    {
        return Err(error(
            "Unsupported stash Delete journal identity; manual review required.",
        ));
    }
    let old = rows(&j.old_log)?;
    let new = rows(&j.new_log)?;
    if old.last().map(|r| r.oid.as_str()) != Some(j.old_tip.as_str())
        || new.last().map(|r| &r.oid) != j.new_tip.as_ref()
        || !old.iter().enumerate().any(|(i, r)| {
            r.oid == j.selected_oid
                && r.entry.message == j.selected_message
                && filtered(&old, i) == j.new_log
        })
    {
        return Err(error(
            "Invalid stash Delete journal transition; manual review required.",
        ));
    }
    Ok(Some((j, bytes)))
}
fn check_journal(repo: &Repository, expected: &Journal) -> AppResult<()> {
    let (actual, _) = read_journal(repo)?.ok_or_else(stale)?;
    if serde_json::to_vec(&actual).ok() != serde_json::to_vec(expected).ok() {
        return Err(stale());
    }
    Ok(())
}
fn journal_matches(j: &Journal, pair: &Pair) -> bool {
    (pair.tip.as_ref() == Some(&j.old_tip)
        && (pair.log == j.old_log || (!j.new_log.is_empty() && pair.log == j.new_log)))
        || (pair.tip == j.new_tip && pair.log == j.new_log)
}
fn safe_metadata_parents(repo: &Repository) -> AppResult<()> {
    for relative in ["logs", "logs/refs", "refs"] {
        let path = repo.common_dir.join(relative);
        let meta = fs::symlink_metadata(&path).map_err(io)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(error(
                "Stash metadata ancestors must be ordinary directories.",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(error("Stash metadata ancestors cannot be reparse points."));
            }
        }
    }
    let reference = repo.common_dir.join("refs/stash");
    if let Ok(m) = fs::symlink_metadata(reference) {
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(error("Stash loose ref must be an ordinary file."));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err(error("Stash loose ref cannot be a reparse point."));
            }
        }
    }
    Ok(())
}
impl GitEngine {
    async fn stash_backup_valid(&self, repo: &Repository, j: &Journal) -> AppResult<bool> {
        let output = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["symbolic-ref", "-q", &j.backup_ref]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if output.code == 0 {
            return Ok(false);
        }
        if output.code != 1 {
            runner::checked(output)?;
        }
        let output = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["rev-parse", "--verify", "--quiet", &j.backup_ref]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?;
        if output.code == 1 {
            return Ok(false);
        }
        Ok(text(&runner::checked(output)?.stdout) == j.selected_oid)
    }
    async fn stash_recovery(&self, repo: &Repository) -> AppResult<Option<StashDeleteRecovery>> {
        let Some((j, _)) = read_journal(repo)? else {
            return Ok(None);
        };
        let pair = self.stash_pair(repo).await?;
        let backup = self.stash_backup_valid(repo, &j).await?;
        let absent = self
            .runner
            .run(
                &repo.root,
                &runner::args(&["show-ref", "--verify", "--quiet", &j.backup_ref]),
                None,
                false,
                runner::STRUCTURED_LIMIT,
            )
            .await?
            .code
            == 1;
        let absent = absent
            && self
                .runner
                .run(
                    &repo.root,
                    &runner::args(&["symbolic-ref", "-q", &j.backup_ref]),
                    None,
                    false,
                    runner::STRUCTURED_LIMIT,
                )
                .await?
                .code
                == 1
            && fs::symlink_metadata(repo.common_dir.join(&j.backup_ref))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        let can_finalize = stash_atomic::supported()
            && journal_matches(&j, &pair)
            && (backup
                || (absent && pair.tip.as_ref() == Some(&j.old_tip) && pair.log == j.old_log));
        let preservation = if backup {
            "The retained backup preserves the selected stash object."
        } else if can_finalize {
            "The original list still retains the selected entry. Finalize creates its backup before deletion."
        } else {
            "The recorded backup is absent or differs; inspect the remaining objects and journal."
        };
        Ok(Some(StashDeleteRecovery {
            recovery_id: j.recovery_id,
            backup_ref: j.backup_ref,
            can_finalize,
            message: format!(
                "Interrupted Delete of {} ({}). {} {} Working files were not reset.",
                j.selected_message,
                j.selected_oid,
                if can_finalize {
                    "Review Finalize to complete the recorded removal."
                } else {
                    "External metadata differs. Preserve the journal and inspect manually; Git View will not repair it automatically."
                },
                preservation
            ),
        }))
    }
    async fn stash_apply_protection(
        &self,
        repo: &Repository,
        oid: &str,
        target: &str,
    ) -> AppResult<()> {
        let shape = self.stash_shape(repo, oid).await?;
        let base = self.tree_paths(repo, &shape.base).await?;
        let incoming = self.tree_paths(repo, oid).await?;
        let target_paths = self.tree_paths(repo, target).await?;
        let mut paths = HashSet::new();
        for tree in [oid, shape.index.as_str()] {
            let bytes = self
                .runner
                .read(
                    &repo.root,
                    &[
                        "--no-replace-objects",
                        "diff",
                        "--name-only",
                        "--no-renames",
                        "-z",
                        &shape.base,
                        tree,
                        "--",
                    ],
                )
                .await?;
            paths.extend(
                bytes
                    .split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .map(<[u8]>::to_vec),
            );
            let raw = self
                .runner
                .read(
                    &repo.root,
                    &[
                        "--no-replace-objects",
                        "diff",
                        "--raw",
                        "--no-abbrev",
                        "--no-renames",
                        "-z",
                        &shape.base,
                        tree,
                        "--",
                    ],
                )
                .await?;
            if raw
                .split(|b| *b == 0)
                .filter_map(|r| r.strip_prefix(b":"))
                .any(|r| {
                    r.split(|b| *b == b' ')
                        .take(2)
                        .any(|mode| mode == b"160000")
                })
            {
                return Err(error("This stash changes a submodule gitlink. Restore it with Git and manage the nested checkout explicitly."));
            }
        }
        if let Some(u) = shape.untracked {
            paths.extend(self.tree_paths(repo, &u).await?);
        }
        self.protect_materialized_paths(repo, &paths).await?;
        let directory = |p: &Vec<u8>| {
            p.rsplitn(2, |b| *b == b'/')
                .nth(1)
                .unwrap_or_default()
                .to_vec()
        };
        let removed: HashSet<_> = base.difference(&target_paths).map(directory).collect();
        let added: HashSet<_> = target_paths.difference(&base).map(directory).collect();
        if incoming.difference(&base).next().is_some()
            && removed.iter().any(|a| added.iter().any(|b| a != b))
        {
            let ignored = self
                .runner
                .read(
                    &repo.root,
                    &[
                        "ls-files",
                        "--others",
                        "--ignored",
                        "--exclude-standard",
                        "--directory",
                        "-z",
                    ],
                )
                .await?;
            if !ignored.is_empty() {
                return Err(AppError::new("ignored_directory_rename","Applying this stash may relocate additions through a directory rename and overwrite ignored content. Move ignored content outside the checkout or restore with Git after preserving it."));
            }
        }
        Ok(())
    }
    pub(crate) async fn prepare_stash(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        action: &RepositoryAction,
    ) -> AppResult<(StashPlan, Vec<OsString>, String)> {
        if !matches!(action, RepositoryAction::FinalizeStashDelete { .. })
            && read_journal(repo)?.is_some()
        {
            return Err(error("An interrupted stash Delete requires reviewed Finalize or manual inspection first."));
        }
        match action {
            RepositoryAction::CreateStash {
                message,
                include_untracked,
                ..
            } => {
                if status.git_state != GitState::Idle || status.counts.conflicted > 0 {
                    return Err(error("Finish the current operation and resolve conflicts before creating a stash."));
                }
                if matches!(status.head, Head::Unborn { .. }) {
                    return Err(error("Create an initial commit before stashing."));
                }
                if status.submodule_dirty
                    || self
                        .observe(repo)
                        .await?
                        .files
                        .iter()
                        .any(|f| f.wire.submodule)
                {
                    return Err(error("Submodule changes are not saved by stash. Resolve or preserve nested changes and gitlink changes separately first."));
                }
                if message.trim().is_empty()
                    || message.len() > 4096
                    || message.contains(['\0', '\r', '\n'])
                {
                    return Err(error(
                        "Use a nonempty stash message up to 4,096 bytes without line breaks.",
                    ));
                }
                if status.counts.staged + status.counts.unstaged == 0
                    && (!include_untracked || status.counts.untracked == 0)
                {
                    return Err(error("There are no eligible changes to save. Untracked files require explicit inclusion."));
                }
                self.stash_pair(repo).await?;
                let mut args = runner::args(&[
                    "-c",
                    "submodule.recurse=false",
                    "stash",
                    "push",
                    "--message",
                    message,
                ]);
                if *include_untracked {
                    args.push("--include-untracked".into());
                }
                Ok((StashPlan::Create,args,format!("Create stash “{message}” from tracked index and working changes{}. Saved changes are removed from this checkout; ignored files and nested submodule work are excluded.",if *include_untracked {" and untracked files"}else{"; leave untracked files in place"})))
            }
            RepositoryAction::ApplyStash {
                stash_snapshot_id,
                entry_id,
                restore_index,
                ..
            } => {
                if status.git_state != GitState::Idle
                    || status.dirty
                    || status.counts.conflicted > 0
                {
                    return Err(error(
                        "Apply requires a clean index and working tree with no ongoing operation.",
                    ));
                }
                let target = match &status.head {
                    Head::Branch { oid, .. } | Head::Detached { oid } => oid,
                    Head::Unborn { .. } => {
                        return Err(error("Apply requires an existing HEAD commit."))
                    }
                };
                let (snapshot, index) = self
                    .stash_selection(repo, stash_snapshot_id, entry_id)
                    .await?;
                let row = snapshot.rows[index].clone();
                self.stash_apply_protection(repo, &row.oid, target).await?;
                let args = runner::args(&[
                    "--no-replace-objects",
                    "-c",
                    "submodule.recurse=false",
                    "stash",
                    "apply",
                    if *restore_index {
                        "--index"
                    } else {
                        "--no-index"
                    },
                    &row.oid,
                ]);
                let summary=format!("Apply stash “{}” ({}) onto {target} and keep its entry. {} Conflicts or other failures may leave partial changes; resolve them externally. There is no stash Abort action.",row.entry.message,row.oid,if *restore_index {"Restore saved index state where Git permits."}else{"Use Git's default index behavior; newly added files can still be staged."});
                Ok((StashPlan::Apply { snapshot, row }, args, summary))
            }
            RepositoryAction::DeleteStash {
                stash_snapshot_id,
                entry_id,
                ..
            } => {
                if !stash_atomic::supported() {
                    return Err(error(
                        "Safe stash Delete is not implemented on this operating system.",
                    ));
                }
                safe_metadata_parents(repo)?;
                let (snapshot, index) = self
                    .stash_selection(repo, stash_snapshot_id, entry_id)
                    .await?;
                let row = &snapshot.rows[index];
                self.runner
                    .read(
                        &repo.root,
                        &["--no-replace-objects", "cat-file", "-e", &row.oid],
                    )
                    .await?;
                let summary=format!("Delete only stash “{}” ({}, recorded {}) from the reviewed list. Working files and index remain untouched. A recovery backup ref under refs/git-view/stash-deletions is retained even after success; this is not secure erasure.",row.entry.message,row.oid,row.entry.created_at);
                Ok((StashPlan::Delete { snapshot, index }, vec![], summary))
            }
            RepositoryAction::FinalizeStashDelete { recovery_id, .. } => {
                let (journal, bytes) = read_journal(repo)?
                    .ok_or_else(|| error("No interrupted stash Delete exists."))?;
                if journal.recovery_id != *recovery_id
                    || !self
                        .stash_recovery(repo)
                        .await?
                        .is_some_and(|r| r.can_finalize)
                {
                    return Err(error("The recorded Delete cannot be finalized automatically. Preserve its journal and backup for manual review."));
                }
                let summary=format!("Finalize the recorded Delete of “{}” ({}). Complete only the exact recorded stash ref/reflog transition; preserve working files and retained backup {}.",journal.selected_message,journal.selected_oid,journal.backup_ref);
                Ok((StashPlan::Finalize { journal, bytes }, vec![], summary))
            }
            _ => Err(error("Invalid stash action.")),
        }
    }
    pub(crate) async fn execute_stash(
        &self,
        plan: &PreparedAction,
        stash: &StashPlan,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        match stash {
            StashPlan::Delete { snapshot, index } => {
                return Box::pin(self.execute_stash_delete(plan, snapshot, *index, canceled)).await
            }
            StashPlan::Finalize { journal, bytes } => {
                if regular_read(&journal_path(repo))? != *bytes {
                    return Err(stale());
                }
                return Box::pin(self.finalize_stash_delete(repo, journal, canceled)).await;
            }
            StashPlan::Apply { snapshot, row } => {
                if snapshot.pair != self.stash_pair(repo).await? {
                    return Err(stale());
                }
                let status = self.observe(repo).await?.status.unwrap();
                let oid = match status.head {
                    Head::Branch { oid, .. } | Head::Detached { oid } => oid,
                    _ => return Err(stale()),
                };
                self.stash_apply_protection(repo, &row.oid, &oid).await?;
            }
            StashPlan::Create => {}
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before stash started."));
        }
        if read_journal(repo)?.is_some() {
            return Err(error("A stash Delete started after preview. Review its interrupted state before creating or applying a stash."));
        }
        let before = self.stash_pair(repo).await?;
        let mut runner = self.runner.clone();
        runner.locale = Some("C".into());
        let outcome = runner
            .run(&repo.root, &plan.args, None, true, runner::DIAGNOSTIC_LIMIT)
            .await;
        let observed = self.observe(repo).await.map_err(|e| {
            uncertain(format!(
                "Stash ran but its resulting working state could not be inspected: {}",
                e.message
            ))
        })?;
        let output=outcome.and_then(runner::checked).map_err(|mut e| {e.message=format!("{} Stash may have left partial index or working changes ({} conflicts). Refresh and inspect; no automatic rollback or retry was performed.",e.message,observed.status.as_ref().unwrap().counts.conflicted);e})?;
        if matches!(stash, StashPlan::Create) {
            let after = self.stash_pair(repo).await.map_err(|e| {
                uncertain(format!(
                    "Create stash ran; inspect its actual list and working files: {}",
                    e.message
                ))
            })?;
            if before.log == after.log
                || output
                    .stdout
                    .windows(b"No local changes to save".len())
                    .any(|s| s == b"No local changes to save")
            {
                return Err(AppError::new("no_stash_created","Git created no stash entry. Inspect remaining changes; nested submodule work is not saved."));
            }
        }
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
}
impl GitEngine {
    async fn stash_guard(
        &self,
        repo: &Repository,
        tip: Option<&str>,
        delete: bool,
    ) -> AppResult<ref_guard::HeadGuard> {
        let body = match tip {
            Some(oid) => format!(
                "option no-deref\n{} refs/stash {oid}\n",
                if delete { "delete" } else { "verify" }
            ),
            None => "option no-deref\nverify refs/stash\n".into(),
        };
        ref_guard::HeadGuard::prepare(&self.runner,&repo.root,&[],body).await.map_err(|mut e|{e.message="Git could not lock the stash reference. Another Git writer or a reference-transaction hook may have prevented the operation. No stash metadata was intentionally changed.".into();e})
    }
    async fn execute_stash_delete(
        &self,
        plan: &PreparedAction,
        snapshot: &StashSnapshot,
        index: usize,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let repo = &plan.repository;
        let singleton = snapshot.rows.len() == 1;
        let guard = self
            .stash_guard(repo, snapshot.pair.tip.as_deref(), singleton)
            .await?;
        if self.stash_pair(repo).await? != snapshot.pair {
            return Err(stale());
        }
        if self.config_fingerprint(repo).await? != plan.config_fingerprint
            || self.observe(repo).await?.fingerprint != *plan.fingerprint.as_ref().unwrap()
        {
            return Err(stale());
        }
        safe_metadata_parents(repo)?;
        if read_journal(repo)?.is_some() {
            return Err(error("A stash Delete journal appeared after preview."));
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before stash Delete."));
        }
        let recovery_id = id();
        let new_log = filtered(&snapshot.rows, index);
        let new_tip = rows(&new_log)?.last().map(|r| r.oid.clone());
        let row = &snapshot.rows[index];
        let journal = Journal {
            version: 1,
            recovery_id: recovery_id.clone(),
            common_dir: repo.common_dir.clone(),
            old_tip: snapshot.pair.tip.clone().ok_or_else(stale)?,
            new_tip,
            old_log: snapshot.pair.log.clone(),
            new_log,
            selected_oid: row.oid.clone(),
            selected_message: row.entry.message.clone(),
            backup_ref: format!("refs/git-view/stash-deletions/{recovery_id}"),
        };
        let bytes = serde_json::to_vec(&journal).map_err(|e| error(e.to_string()))?;
        if bytes.len() > LIMIT {
            return Err(error("Stash Delete journal exceeds its bounded size."));
        }
        write_new(&journal_path(repo), &bytes)?;
        self.stash_fault("journal")?;
        let zero = "0".repeat(journal.selected_oid.len());
        runner::checked(
            self.runner
                .run(
                    &repo.root,
                    &runner::args(&[
                        "update-ref",
                        "--no-deref",
                        &journal.backup_ref,
                        &journal.selected_oid,
                        &zero,
                    ]),
                    None,
                    true,
                    runner::DIAGNOSTIC_LIMIT,
                )
                .await?,
        )?;
        self.stash_fault("backup")?;
        if !self.stash_backup_valid(repo, &journal).await? {
            return Err(uncertain(
                "Stash backup was not preserved as the expected direct ref. Inspect the journal.",
            ));
        }
        self.finish_stash_transition(repo, &journal, guard, singleton)
            .await
            .map_err(|mut e| {
                e.code = "outcome_uncertain".into();
                e.message = format!(
                    "{} Inspect the retained stash Delete journal and backup before retrying.",
                    e.message
                );
                e
            })
    }
    async fn finalize_stash_delete(
        &self,
        repo: &Repository,
        journal: &Journal,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        let before = self.stash_pair(repo).await?;
        if !journal_matches(journal, &before)
            || !self
                .stash_recovery(repo)
                .await?
                .is_some_and(|r| r.can_finalize)
        {
            return Err(error(
                "Stash metadata or backup changed; manual review is required.",
            ));
        }
        let singleton = journal.new_tip.is_none() && before.tip.is_some();
        let guard = self
            .stash_guard(repo, before.tip.as_deref(), singleton)
            .await?;
        check_journal(repo, journal)?;
        if self.stash_pair(repo).await? != before {
            return Err(stale());
        }
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new("canceled", "Canceled before Finalize."));
        }
        if !self.stash_backup_valid(repo, journal).await? {
            let zero = "0".repeat(journal.selected_oid.len());
            runner::checked(
                self.runner
                    .run(
                        &repo.root,
                        &runner::args(&[
                            "update-ref",
                            "--no-deref",
                            &journal.backup_ref,
                            &journal.selected_oid,
                            &zero,
                        ]),
                        None,
                        true,
                        runner::DIAGNOSTIC_LIMIT,
                    )
                    .await?,
            )?;
        }
        self.finish_stash_transition(repo, journal, guard, singleton).await.map_err(|mut e| {e.code="outcome_uncertain".into();e.message=format!("{} Finalize may have changed stash metadata; inspect the retained recovery record.",e.message);e})
    }
    async fn finish_stash_transition(
        &self,
        repo: &Repository,
        j: &Journal,
        guard: ref_guard::HeadGuard,
        singleton: bool,
    ) -> AppResult<ActionOutcome> {
        // From the first metadata write onward, errors preserve the durable journal and
        // retained backup. No read path attempts rollback or repairs external changes.
        check_journal(repo, j)?;
        let backup_guard = ref_guard::HeadGuard::prepare(
            &self.runner,
            &repo.root,
            &[],
            format!(
                "option no-deref\nverify {} {}\n",
                j.backup_ref, j.selected_oid
            ),
        )
        .await?;
        let before = self.stash_pair(repo).await?;
        if !journal_matches(j, &before) || !self.stash_backup_valid(repo, j).await? {
            return Err(uncertain("Stash metadata changed under its guard. Preserve the journal and inspect manually."));
        }
        check_journal(repo, j)?;
        if singleton {
            if before.log != j.old_log || before.tip.as_ref() != Some(&j.old_tip) {
                return Err(stale());
            }
            guard.commit().await?;
            self.stash_fault("singleton")?;
        } else {
            if before.log == j.old_log {
                safe_metadata_parents(repo)?;
                let log = log_path(repo);
                let parent = log.parent().unwrap();
                let new = parent.join(format!(".git-view-stash-{}-new", j.recovery_id));
                let old = parent.join(format!(".git-view-stash-{}-old", j.recovery_id));
                if new.exists() {
                    if regular_read(&new)? != j.new_log {
                        return Err(error("Retained replacement file differs; inspect the journal and metadata manually."));
                    }
                } else {
                    write_new(&new, &j.new_log)?;
                }
                // Windows FlushFileBuffers requires a write-capable handle. Open
                // without truncation, then keep that handle through permission
                // preservation and flush; a read-only File::open is insufficient.
                let step_error = |step: &str, cause: std::io::Error| {
                    error(format!(
                        "Could not {step} for stash replacement {}: {cause}",
                        new.display()
                    ))
                };
                let replacement = fs::OpenOptions::new()
                    .write(true)
                    .open(&new)
                    .map_err(|e| step_error("open a writable handle", e))?;
                let permissions = fs::metadata(&log)
                    .map_err(|e| step_error("read original reflog permissions", e))?
                    .permissions();
                replacement
                    .set_permissions(permissions)
                    .map_err(|e| step_error("preserve reflog permissions", e))?;
                replacement
                    .sync_all()
                    .map_err(|e| step_error("flush the writable handle", e))?;
                drop(replacement);
                // A fresh comparison plus a REQUIRE-EXISTING replacement prevents the
                // native reflog-drop exception from resurrecting an unlinked reflog.
                if self.stash_pair(repo).await? != before {
                    return Err(stale());
                }
                check_journal(repo, j)?;
                self.stash_fault("beforeLog")?;
                #[cfg(test)]
                if self.stash_failure.lock().unwrap().as_deref() == Some("dropBeforeLog") {
                    runner::checked(
                        self.runner
                            .run(
                                &repo.root,
                                &runner::args(&["reflog", "drop", "refs/stash"]),
                                None,
                                true,
                                runner::DIAGNOSTIC_LIMIT,
                            )
                            .await?,
                    )?;
                }
                stash_atomic::replace_existing(&new,&log,&old).map_err(|e|uncertain(format!("Conditional stash reflog replacement failed: {e}. The journal and any old/replacement files were retained for inspection.")))?;
                sync_directory(parent)?;
                if regular_read(&old)? != j.old_log {
                    return Err(uncertain("The replaced reflog was not the reviewed file. Both versions were preserved; manual review is required."));
                }
                self.stash_fault("log")?;
                #[cfg(test)]
                if self.stash_failure.lock().unwrap().as_deref() == Some("dropAfterLog") {
                    runner::checked(
                        self.runner
                            .run(
                                &repo.root,
                                &runner::args(&["reflog", "drop", "refs/stash"]),
                                None,
                                true,
                                runner::DIAGNOSTIC_LIMIT,
                            )
                            .await?,
                    )?;
                }
            }
            let current = self.stash_pair(repo).await?;
            if current.log != j.new_log
                || ![Some(&j.old_tip), j.new_tip.as_ref()].contains(&current.tip.as_ref())
            {
                return Err(uncertain("The stash ref or reflog changed during Delete; no further repair was attempted."));
            }
            if current.tip != j.new_tip {
                check_journal(repo, j)?;
                safe_metadata_parents(repo)?;
                let path = repo.common_dir.join("refs/stash");
                let mut file =
                    tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(io)?;
                file.write_all(
                    format!(
                        "{}\n",
                        j.new_tip
                            .as_ref()
                            .ok_or_else(|| error("Invalid empty replacement ref."))?
                    )
                    .as_bytes(),
                )
                .map_err(io)?;
                if let Ok(meta) = fs::metadata(&path)
                    .or_else(|_| fs::metadata(repo.common_dir.join("refs/stash.lock")))
                {
                    file.as_file()
                        .set_permissions(meta.permissions())
                        .map_err(io)?;
                }
                file.as_file().sync_all().map_err(io)?;
                file.persist(&path).map_err(|e| io(e.error))?;
                sync_directory(path.parent().unwrap())?;
                self.stash_fault("ref")?;
            }
            let pair = self.stash_pair(repo).await?;
            if pair.tip != j.new_tip || pair.log != j.new_log {
                return Err(uncertain("The resulting stash list differs from the recorded Delete. Inspect the retained journal and backup."));
            }
            guard.release().await?;
        }
        backup_guard.release().await?;
        self.stash_fault("release")?;
        let pair = self.stash_pair(repo).await?;
        if pair.tip != j.new_tip || pair.log != j.new_log {
            return Err(uncertain("External stash activity changed the result after its lock was released. The Delete journal and backup were retained."));
        }
        let Some((current, bytes)) = read_journal(repo)? else {
            return Err(uncertain(
                "Stash Delete completed but its journal disappeared.",
            ));
        };
        if serde_json::to_vec(&current).ok() != serde_json::to_vec(j).ok() {
            return Err(uncertain(
                "Stash Delete completed but its journal changed; external bytes were preserved.",
            ));
        }
        // Keep the entire historical record beside its backup; list/status only sees
        // the active journal. The next explicit Delete cannot overwrite this archive.
        let archive = repo.common_dir.join(format!(
            "git-view-stash-delete-{}.completed.json",
            j.recovery_id
        ));
        if archive.exists() {
            if regular_read(&archive)? != bytes {
                return Err(uncertain("A different completed Delete archive exists; preserve all metadata for inspection."));
            }
        } else {
            write_new(&archive, &bytes)?;
        }
        self.stash_fault("archive")?;
        check_journal(repo, j)?;
        fs::remove_file(journal_path(repo)).map_err(io)?;
        sync_directory(&repo.common_dir)?;
        Ok(ActionOutcome {
            exit_code: 0,
            output_truncated: false,
        })
    }
    fn stash_fault(&self, _phase: &str) -> AppResult<()> {
        #[cfg(test)]
        if self.stash_failure.lock().unwrap().as_deref() == Some(_phase) {
            return Err(uncertain(format!(
                "Injected stash interruption at {_phase}"
            )));
        }
        Ok(())
    }
}
#[cfg(all(test, any(target_os = "macos", windows)))]
mod tests {
    use super::*;
    fn git(path: &Path, args: &[&str]) -> String {
        let null = "/dev/null";
        let mut command = std::process::Command::new("git");
        command
            .current_dir(path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null)
            .env("GIT_CONFIG_SYSTEM", null);
        for key in [
            "GIT_TEMPLATE_DIR",
            "GIT_EXEC_PATH",
            "GIT_CONFIG",
            "GIT_NAMESPACE",
            "GIT_SHALLOW_FILE",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_DATE",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_DATE",
            "EMAIL",
        ] {
            command.env_remove(key);
        }
        let output = command.args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout)
    }
    async fn fixture(singleton: bool) -> (tempfile::TempDir, GitEngine, String) {
        fixture_format(singleton, false).await
    }
    async fn fixture_format(
        singleton: bool,
        sha256: bool,
    ) -> (tempfile::TempDir, GitEngine, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        if sha256 {
            git(p, &["init", "--object-format=sha256", "-b", "main"]);
        } else {
            git(p, &["init", "-b", "main"]);
        }
        git(p, &["config", "user.name", "Fixture"]);
        git(p, &["config", "user.email", "fixture@example.invalid"]);
        git(p, &["config", "commit.gpgsign", "false"]);
        git(p, &["config", "core.hooksPath", ".disabled-hooks"]);
        fs::write(p.join("file"), "base").unwrap();
        git(p, &["add", "file"]);
        git(p, &["commit", "-m", "base"]);
        for i in 0..if singleton { 1 } else { 3 } {
            fs::write(p.join("file"), format!("saved {i}")).unwrap();
            git(p, &["stash", "push", "-m", &format!("entry {i}")]);
        }
        let engine = GitEngine::isolated();
        let id = engine.open_repository(p).await.unwrap().repository_id;
        (dir, engine, id)
    }
    async fn deletion(engine: &GitEngine, id: &str) -> PreparedAction {
        let list = engine.list_stashes(id).await.unwrap();
        let status = engine.get_status(id).await.unwrap();
        engine
            .prepare_action(
                id,
                RepositoryAction::DeleteStash {
                    status_snapshot_id: status.snapshot_id,
                    stash_snapshot_id: list.snapshot_id,
                    entry_id: list.entries[0].entry_id.clone(),
                },
            )
            .await
            .unwrap()
    }
    async fn finalize(engine: &GitEngine, id: &str) -> AppResult<ActionOutcome> {
        let recovery = engine.list_stashes(id).await?.recovery.unwrap();
        assert!(recovery.can_finalize, "{}", recovery.message);
        let status = engine.get_status(id).await?;
        let plan = engine
            .prepare_action(
                id,
                RepositoryAction::FinalizeStashDelete {
                    status_snapshot_id: status.snapshot_id,
                    recovery_id: recovery.recovery_id,
                },
            )
            .await?;
        engine.execute(&plan).await
    }
    #[tokio::test]
    async fn interruptions_are_read_only_until_explicit_finalize() {
        for phase in [
            "journal",
            "backup",
            "beforeLog",
            "log",
            "ref",
            "release",
            "archive",
        ] {
            let (dir, engine, id) = fixture(false).await;
            let p = dir.path();
            let plan = deletion(&engine, &id).await;
            *engine.stash_failure.lock().unwrap() = Some(phase.into());
            assert!(engine.execute(&plan).await.is_err(), "{phase}");
            let before = fs::read(p.join(".git/logs/refs/stash")).unwrap();
            let journal = fs::read(p.join(".git/git-view-stash-delete.json")).unwrap();
            let restarted = GitEngine::isolated();
            let id = restarted.open_repository(p).await.unwrap().repository_id;
            let listed = restarted.list_stashes(&id).await.unwrap();
            assert!(listed.recovery.as_ref().unwrap().can_finalize, "{phase}");
            assert_eq!(fs::read(p.join(".git/logs/refs/stash")).unwrap(), before);
            assert_eq!(
                fs::read(p.join(".git/git-view-stash-delete.json")).unwrap(),
                journal
            );
            finalize(&restarted, &id)
                .await
                .unwrap_or_else(|e| panic!("Finalize after {phase}: {e:?}"));
            let after = restarted.list_stashes(&id).await.unwrap();
            assert!(after.recovery.is_none());
            assert_eq!(after.entries.len(), 2, "{phase}");
            assert!(after
                .entries
                .iter()
                .all(|e| !e.message.ends_with("entry 2")));
            assert_eq!(fs::read_to_string(p.join("file")).unwrap(), "base");
        }
    }
    #[tokio::test]
    async fn singleton_native_delete_is_recoverable_after_commit() {
        let (dir, engine, id) = fixture(true).await;
        let plan = deletion(&engine, &id).await;
        *engine.stash_failure.lock().unwrap() = Some("singleton".into());
        assert!(engine.execute(&plan).await.is_err());
        assert!(!dir.path().join(".git/logs/refs/stash").exists());
        let restart = GitEngine::isolated();
        let id = restart
            .open_repository(dir.path())
            .await
            .unwrap()
            .repository_id;
        finalize(&restart, &id).await.unwrap();
        assert!(restart.list_stashes(&id).await.unwrap().entries.is_empty());
    }
    #[tokio::test]
    async fn concurrent_native_reflog_drop_is_never_resurrected() {
        for phase in ["dropBeforeLog", "dropAfterLog"] {
            let (dir, engine, id) = fixture(false).await;
            let plan = deletion(&engine, &id).await;
            *engine.stash_failure.lock().unwrap() = Some(phase.into());
            let failure = engine.execute(&plan).await.err().unwrap();
            assert_eq!(failure.code, "outcome_uncertain", "{phase}: {failure:?}");
            assert!(
                !dir.path().join(".git/logs/refs/stash").exists(),
                "{phase} was not reached or the log was recreated: {failure:?}"
            );
            let list = engine.list_stashes(&id).await.unwrap();
            assert!(!list.recovery.unwrap().can_finalize);
            assert!(dir.path().join(".git/git-view-stash-delete.json").exists());
        }
    }
    #[tokio::test]
    async fn external_changes_and_journal_drift_refuse_recovery_without_writing() {
        let (dir, engine, id) = fixture(false).await;
        let plan = deletion(&engine, &id).await;
        *engine.stash_failure.lock().unwrap() = Some("log".into());
        assert!(engine.execute(&plan).await.is_err());
        *engine.stash_failure.lock().unwrap() = None;
        let recovery = engine.list_stashes(&id).await.unwrap().recovery.unwrap();
        let status = engine.get_status(&id).await.unwrap();
        let plan = engine
            .prepare_action(
                &id,
                RepositoryAction::FinalizeStashDelete {
                    status_snapshot_id: status.snapshot_id,
                    recovery_id: recovery.recovery_id,
                },
            )
            .await
            .unwrap();
        let path = dir.path().join(".git/git-view-stash-delete.json");
        let bytes = b"external corrupt journal";
        fs::write(&path, bytes).unwrap();
        assert!(engine.execute(&plan).await.is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    #[tokio::test]
    async fn sha256_singleton_recovery_and_stale_other_engine_journal() {
        let (dir, engine, id) = fixture_format(true, true).await;
        let plan = deletion(&engine, &id).await;
        *engine.stash_failure.lock().unwrap() = Some("singleton".into());
        assert!(engine.execute(&plan).await.is_err());
        let restarted = GitEngine::isolated();
        let id = restarted
            .open_repository(dir.path())
            .await
            .unwrap()
            .repository_id;
        finalize(&restarted, &id).await.unwrap();
        assert!(restarted
            .list_stashes(&id)
            .await
            .unwrap()
            .entries
            .is_empty());
        let (dir, engine, id) = fixture(false).await;
        fs::write(dir.path().join("file"), "next changes").unwrap();
        let status = engine.get_status(&id).await.unwrap();
        let plan = engine
            .prepare_action(
                &id,
                RepositoryAction::CreateStash {
                    status_snapshot_id: status.snapshot_id,
                    message: "stale create".into(),
                    include_untracked: false,
                },
            )
            .await
            .unwrap();
        let other = GitEngine::isolated();
        let other_id = other
            .open_repository(dir.path())
            .await
            .unwrap()
            .repository_id;
        let delete = deletion(&other, &other_id).await;
        *other.stash_failure.lock().unwrap() = Some("journal".into());
        assert!(other.execute(&delete).await.is_err());
        let log = fs::read(dir.path().join(".git/logs/refs/stash")).unwrap();
        let journal = fs::read(dir.path().join(".git/git-view-stash-delete.json")).unwrap();
        assert_eq!(
            engine.execute(&plan).await.err().unwrap().code,
            "stash_unavailable"
        );
        assert_eq!(
            fs::read(dir.path().join(".git/logs/refs/stash")).unwrap(),
            log
        );
        assert_eq!(
            fs::read(dir.path().join(".git/git-view-stash-delete.json")).unwrap(),
            journal
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("file")).unwrap(),
            "next changes"
        );
    }
    #[tokio::test]
    async fn prepared_hook_journal_changes_are_preserved_and_not_installed() {
        let (dir, engine, id) = fixture(false).await;
        let plan = deletion(&engine, &id).await;
        *engine.stash_failure.lock().unwrap() = Some("log".into());
        assert!(engine.execute(&plan).await.is_err());
        *engine.stash_failure.lock().unwrap() = None;
        let p = dir.path();
        let hook = p.join(".git/hooks/reference-transaction");
        fs::write(&hook,b"#!/bin/sh\n[ \"$1\" = prepared ] || exit 0\nwhile read -r old new ref; do\n if [ \"$ref\" = refs/stash ]; then\n  printf 'external journal mutation' > \"$(git rev-parse --git-common-dir)/git-view-stash-delete.json\"\n fi\ndone\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(p, &["config", "core.hooksPath", ".git/hooks"]);
        let before = fs::read(p.join(".git/logs/refs/stash")).unwrap();
        let tip = git(p, &["rev-parse", "refs/stash"]);
        assert!(finalize(&engine, &id).await.is_err());
        assert_eq!(fs::read(p.join(".git/logs/refs/stash")).unwrap(), before);
        assert_eq!(git(p, &["rev-parse", "refs/stash"]), tip);
        assert_eq!(
            fs::read(p.join(".git/git-view-stash-delete.json")).unwrap(),
            b"external journal mutation"
        );
    }
}
