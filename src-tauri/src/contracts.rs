use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_truncated: Option<bool>,
}
impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
            details: None,
            exit_code: None,
            output_truncated: None,
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Head {
    Branch { name: String, oid: String },
    Detached { oid: String },
    Unborn { name: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitState {
    Idle,
    Merge,
    SquashMerge,
    Rebase,
    CherryPick,
    Revert,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationStopReason {
    Conflicts,
    Empty,
    Ready,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationContext {
    pub kind: GitState,
    pub source_oids: Vec<String>,
    pub source_ref: Option<String>,
    pub original_head: Option<String>,
    pub target_branch: Option<String>,
    pub stop_reason: OperationStopReason,
    pub can_continue: bool,
    pub can_abort: bool,
    pub can_skip: bool,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebase: Option<RebaseProgress>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseProgress {
    pub onto_ref: Option<String>,
    pub onto_oid: Option<String>,
    pub current_oid: Option<String>,
    pub completed: Option<usize>,
    pub total: Option<usize>,
    pub app_owned: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upstream {
    #[serde(rename = "ref")]
    pub reference: String,
    pub remote: String,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryStatus {
    pub repository_id: String,
    pub snapshot_id: String,
    pub observed_at: String,
    pub head: Head,
    pub git_state: GitState,
    pub dirty: bool,
    pub counts: Counts,
    pub submodule_dirty: bool,
    pub upstream: Option<Upstream>,
    pub last_successful_fetch_at: Option<String>,
    pub active_operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squash_merge: Option<SquashMergeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_context: Option<OperationContext>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SquashMergeState {
    pub original_head: String,
    pub source_oid: String,
    pub source_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryRow {
    Ready {
        member_id: String,
        repository_id: String,
        label: String,
        display_path: String,
        status: RepositoryStatus,
    },
    Loading {
        member_id: String,
        label: String,
        display_path: String,
    },
    Unavailable {
        member_id: String,
        label: String,
        display_path: String,
        error: AppError,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryOpen {
    pub repository_id: String,
    pub display_path: String,
    pub status: RepositoryStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub file_id: String,
    pub display_path: String,
    pub old_display_path: Option<String>,
    pub change_kind: Option<String>,
    pub index_status: Option<String>,
    pub worktree_status: Option<String>,
    pub untracked: bool,
    pub conflicted: bool,
    pub submodule: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub snapshot_id: String,
    pub files: Vec<ChangedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefDecoration {
    pub full_name: String,
    pub label: String,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRef {
    pub full_name: String,
    pub label: String,
    pub oid: String,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCheckoutTarget {
    pub remote_id: String,
    pub remote_name: String,
    pub source_ref: String,
    pub source_oid: String,
    pub upstream_ref: String,
    pub suggested_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranchIssue {
    pub source_ref: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranchListing {
    pub targets: Vec<RemoteCheckoutTarget>,
    pub issues: Vec<RemoteBranchIssue>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub entry_id: String,
    pub oid: String,
    pub message: String,
    pub created_at: String,
    pub branch: Option<String>,
    pub has_untracked: bool,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashDeleteRecovery {
    pub recovery_id: String,
    pub message: String,
    pub can_finalize: bool,
    pub backup_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashList {
    pub snapshot_id: String,
    pub entries: Vec<StashEntry>,
    pub delete_unavailable_reason: Option<String>,
    pub recovery: Option<StashDeleteRecovery>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StashSection {
    Worktree,
    Index,
    Untracked,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub id: String,
    pub name: String,
    pub default_fetch: bool,
    pub default_push: bool,
    pub destination_ref: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub oid: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub authored_at: String,
    pub refs: Vec<RefDecoration>,
    pub boundary: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub snapshot_id: String,
    pub commits: Vec<CommitSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileHistorySource {
    pub snapshot_id: String,
    pub file_id: String,
    pub path: FileHistoryPath,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileHistoryPath {
    Current,
    Previous,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileHistoryRequest {
    pub repository_id: String,
    pub source: FileHistorySource,
    pub history_snapshot_id: Option<String>,
    pub cursor: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryPage {
    pub display_path: String,
    pub start_oid: Option<String>,
    pub shallow: bool,
    pub history: HistoryPage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlameRequest {
    pub repository_id: String,
    pub snapshot_id: String,
    pub file_id: String,
    pub side: FileHistoryPath,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    pub line: usize,
    pub original_line: usize,
    pub oid: String,
    pub author: Identity,
    pub subject: String,
    pub original_path: String,
    pub boundary: bool,
    pub content: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameResult {
    pub snapshot_id: String,
    pub revision_oid: Option<String>,
    pub display_path: String,
    pub working: bool,
    pub shallow: bool,
    pub unavailable_reason: Option<String>,
    pub lines: Vec<BlameLine>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistorySearch {
    pub field: HistorySearchField,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistorySearchField {
    Message,
    Author,
    Oid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: String,
    pub email: String,
    pub at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub snapshot_id: String,
    pub oid: String,
    pub parents: Vec<String>,
    pub selected_parent: Option<String>,
    pub message: String,
    pub author: Identity,
    pub committer: Identity,
    pub files: Vec<ChangedFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest {
    pub repository_id: String,
    pub snapshot_id: String,
    pub file_id: String,
    pub context: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiffResult {
    Text {
        text: String,
        truncated: bool,
        #[serde(
            rename = "hunkActions",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        hunk_actions: Option<HunkActions>,
    },
    Binary {
        description: String,
    },
    Unavailable {
        reason: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HunkActions {
    Available {
        diff_snapshot_id: String,
        context: HunkContext,
        preamble: String,
        hunks: Vec<DiffHunk>,
    },
    Unavailable {
        reason: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HunkContext {
    Staged,
    Unstaged,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub hunk_id: String,
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepositoryAction {
    Stage {
        snapshot_id: String,
        file_ids: Vec<String>,
    },
    Unstage {
        snapshot_id: String,
        file_ids: Vec<String>,
    },
    StageHunk {
        diff_snapshot_id: String,
        file_id: String,
        hunk_id: String,
    },
    UnstageHunk {
        diff_snapshot_id: String,
        file_id: String,
        hunk_id: String,
    },
    Commit {
        status_snapshot_id: String,
        message: String,
    },
    Fetch {
        remote_id: String,
    },
    Pull {
        status_snapshot_id: String,
    },
    Push {
        status_snapshot_id: String,
        remote_id: String,
        destination_ref: String,
        set_upstream: bool,
    },
    CreateBranch {
        status_snapshot_id: String,
        name: String,
        start_oid: String,
        switch_after_create: bool,
    },
    SwitchBranch {
        status_snapshot_id: String,
        local_ref: String,
    },
    CheckoutRemoteBranch {
        status_snapshot_id: String,
        remote_id: String,
        source_ref: String,
        name: String,
    },
    Merge {
        status_snapshot_id: String,
        source_ref: String,
    },
    SquashMerge {
        status_snapshot_id: String,
        source_ref: String,
    },
    ContinueMerge {
        status_snapshot_id: String,
    },
    AbortMerge {
        status_snapshot_id: String,
    },
    CherryPick {
        status_snapshot_id: String,
        source_oid: String,
    },
    ContinueCherryPick {
        status_snapshot_id: String,
    },
    AbortCherryPick {
        status_snapshot_id: String,
    },
    SkipCherryPick {
        status_snapshot_id: String,
    },
    StartRebase {
        status_snapshot_id: String,
        onto_ref: String,
    },
    ContinueRebase {
        status_snapshot_id: String,
    },
    SkipRebase {
        status_snapshot_id: String,
    },
    AbortRebase {
        status_snapshot_id: String,
    },
    CreateStash {
        status_snapshot_id: String,
        message: String,
        include_untracked: bool,
    },
    ApplyStash {
        status_snapshot_id: String,
        stash_snapshot_id: String,
        entry_id: String,
        restore_index: bool,
    },
    DeleteStash {
        status_snapshot_id: String,
        stash_snapshot_id: String,
        entry_id: String,
    },
    FinalizeStashDelete {
        status_snapshot_id: String,
        recovery_id: String,
    },
    CombineCommits {
        status_snapshot_id: String,
        commit_oids: Vec<String>,
        message: String,
    },
    RecoverCombine {
        status_snapshot_id: String,
        recovery_id: String,
    },
}
impl RepositoryAction {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Stage { .. } => "stage",
            Self::Unstage { .. } => "unstage",
            Self::StageHunk { .. } => "stageHunk",
            Self::UnstageHunk { .. } => "unstageHunk",
            Self::Commit { .. } => "commit",
            Self::Fetch { .. } => "fetch",
            Self::Pull { .. } => "pull",
            Self::Push { .. } => "push",
            Self::CreateBranch { .. } => "createBranch",
            Self::SwitchBranch { .. } => "switchBranch",
            Self::CheckoutRemoteBranch { .. } => "checkoutRemoteBranch",
            Self::Merge { .. } => "merge",
            Self::SquashMerge { .. } => "squashMerge",
            Self::ContinueMerge { .. } => "continueMerge",
            Self::AbortMerge { .. } => "abortMerge",
            Self::CherryPick { .. } => "cherryPick",
            Self::ContinueCherryPick { .. } => "continueCherryPick",
            Self::AbortCherryPick { .. } => "abortCherryPick",
            Self::SkipCherryPick { .. } => "skipCherryPick",
            Self::StartRebase { .. } => "startRebase",
            Self::ContinueRebase { .. } => "continueRebase",
            Self::SkipRebase { .. } => "skipRebase",
            Self::AbortRebase { .. } => "abortRebase",
            Self::CreateStash { .. } => "createStash",
            Self::ApplyStash { .. } => "applyStash",
            Self::DeleteStash { .. } => "deleteStash",
            Self::FinalizeStashDelete { .. } => "finalizeStashDelete",
            Self::CombineCommits { .. } => "combineCommits",
            Self::RecoverCombine { .. } => "recoverCombine",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteRecord {
    pub recovery_id: String,
    pub phase: String,
    pub branch: String,
    pub original_head: String,
    pub replacement_head: Option<String>,
    pub base_oid: String,
    pub tree_oid: String,
    pub backup_ref: String,
    pub replacement_ref: Option<String>,
    pub commit_oids: Vec<String>,
    pub message: String,
    pub created_at: String,
    pub error: Option<String>,
    pub can_recover: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RepositoryActionPlan {
    Eligible {
        plan_id: String,
        expires_at: String,
        summary: String,
        destination: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rebase_preview: Option<RebasePreview>,
    },
    Blocked {
        reason: String,
        error: Option<AppError>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePreview {
    pub branch: String,
    pub original_head: String,
    pub onto_ref: String,
    pub onto_oid: String,
    pub base_oid: String,
    pub commits: Vec<RebaseCommit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseCommit {
    pub oid: String,
    pub subject: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Stopped,
    Failed,
    Skipped,
    Canceled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub operation_id: String,
    pub batch_id: Option<String>,
    pub member_id: Option<String>,
    pub repository_id: Option<String>,
    pub action: String,
    pub state: JobState,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub error: Option<AppError>,
    pub output_truncated: bool,
    pub outcome_uncertain: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMember {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectContent {
    pub version: u32,
    pub name: String,
    pub repositories: Vec<ProjectMember>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub project_id: String,
    pub revision: String,
    pub display_path: String,
    pub name: String,
    pub content: ProjectContent,
    pub members: Vec<RepositoryRow>,
    pub stale: bool,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRequest {
    pub project_id: String,
    pub revision: String,
    pub member_ids: Vec<String>,
    pub action: String,
    pub branch_filter: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCandidate {
    pub member_id: String,
    pub repository_id: Option<String>,
    pub label: String,
    pub eligible: bool,
    pub reason: Option<String>,
    pub destination: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPlan {
    pub plan_id: String,
    pub expires_at: String,
    pub candidates: Vec<BatchCandidate>,
    pub action: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub batch_id: String,
    pub operation_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub request_id: String,
    pub kind: String,
    pub path: Option<String>,
    pub error: Option<AppError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_depth: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryCandidate {
    pub repository_id: String,
    pub display_path: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryIssue {
    pub path: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResult {
    pub root_path: String,
    pub depth: u8,
    pub candidates: Vec<DiscoveryCandidate>,
    pub issues: Vec<DiscoveryIssue>,
    pub truncated: bool,
    pub visited_directories: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_renderer_fixtures_deserialize() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../src/test/fixtures/wire.json")).unwrap();
        for name in [
            "clean",
            "mixed",
            "detached",
            "unborn",
            "conflict",
            "missingUpstream",
            "unknownCounts",
        ] {
            let status: RepositoryStatus = serde_json::from_value(v[name].clone()).unwrap();
            assert_eq!(serde_json::to_value(status).unwrap(), v[name]);
        }
        serde_json::from_value::<RepositoryRow>(v["missingMember"].clone()).unwrap();
        serde_json::from_value::<DiffResult>(v["binaryDiff"].clone()).unwrap();
        serde_json::from_value::<OperationResult>(v["failedAction"].clone()).unwrap();
        serde_json::from_value::<CommitDetails>(v["mergeCommit"].clone()).unwrap();
        serde_json::from_value::<HistoryPage>(v["history"].clone()).unwrap();
    }
}
