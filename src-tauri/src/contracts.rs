use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError { pub code: String, pub message: String, pub retryable: bool, #[serde(skip_serializing_if = "Option::is_none")] pub details: Option<String> }
impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self { Self { code: code.into(), message: message.into(), retryable: false, details: None } }
}
impl std::fmt::Display for AppError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.message) } }
impl std::error::Error for AppError {}
pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Head { Branch { name: String, oid: String }, Detached { oid: String }, Unborn { name: String } }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitState { Idle, Merge, Rebase, CherryPick, Revert, Other }
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts { pub staged: usize, pub unstaged: usize, pub untracked: usize, pub conflicted: usize }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upstream { #[serde(rename = "ref")] pub reference: String, pub remote: String, pub ahead: Option<u64>, pub behind: Option<u64> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryStatus { pub repository_id: String, pub snapshot_id: String, pub observed_at: String, pub head: Head, pub git_state: GitState, pub dirty: bool, pub counts: Counts, pub submodule_dirty: bool, pub upstream: Option<Upstream>, pub last_successful_fetch_at: Option<String>, pub active_operation_id: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum RepositoryRow {
    Ready { member_id: String, repository_id: String, label: String, display_path: String, status: RepositoryStatus },
    Loading { member_id: String, label: String, display_path: String },
    Unavailable { member_id: String, label: String, display_path: String, error: AppError },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryOpen { pub repository_id: String, pub display_path: String, pub status: RepositoryStatus }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile { pub file_id: String, pub display_path: String, pub old_display_path: Option<String>, pub change_kind: Option<String>, pub index_status: Option<String>, pub worktree_status: Option<String>, pub untracked: bool, pub conflicted: bool, pub submodule: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes { pub snapshot_id: String, pub files: Vec<ChangedFile> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefDecoration { pub full_name: String, pub label: String, pub kind: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRef { pub full_name: String, pub label: String, pub oid: String, pub kind: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote { pub id: String, pub name: String, pub default_fetch: bool, pub default_push: bool, pub destination_ref: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary { pub oid: String, pub parents: Vec<String>, pub subject: String, pub author_name: String, pub authored_at: String, pub refs: Vec<RefDecoration>, pub boundary: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage { pub snapshot_id: String, pub commits: Vec<CommitSummary>, pub next_cursor: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity { pub name: String, pub email: String, pub at: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails { pub snapshot_id: String, pub oid: String, pub parents: Vec<String>, pub selected_parent: Option<String>, pub message: String, pub author: Identity, pub committer: Identity, pub files: Vec<ChangedFile> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest { pub repository_id: String, pub snapshot_id: String, pub file_id: String, pub context: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiffResult { Text { text: String, truncated: bool }, Binary { description: String }, Unavailable { reason: String } }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum RepositoryAction {
    Stage { snapshot_id: String, file_ids: Vec<String> },
    Unstage { snapshot_id: String, file_ids: Vec<String> },
    Commit { status_snapshot_id: String, message: String },
    Fetch { remote_id: String },
    Pull { status_snapshot_id: String },
    Push { status_snapshot_id: String, remote_id: String, destination_ref: String, set_upstream: bool },
    CreateBranch { status_snapshot_id: String, name: String, start_oid: String, switch_after_create: bool },
    SwitchBranch { status_snapshot_id: String, local_ref: String },
    Merge { status_snapshot_id: String, source_ref: String },
    SquashMerge { status_snapshot_id: String, source_ref: String },
    ContinueMerge { status_snapshot_id: String },
    AbortMerge { status_snapshot_id: String },
}
impl RepositoryAction { pub fn kind(&self) -> &'static str { match self { Self::Stage {..}=>"stage",Self::Unstage {..}=>"unstage",Self::Commit {..}=>"commit",Self::Fetch {..}=>"fetch",Self::Pull {..}=>"pull",Self::Push {..}=>"push",Self::CreateBranch {..}=>"createBranch",Self::SwitchBranch {..}=>"switchBranch",Self::Merge {..}=>"merge",Self::SquashMerge {..}=>"squashMerge",Self::ContinueMerge {..}=>"continueMerge",Self::AbortMerge {..}=>"abortMerge" } } }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RepositoryActionPlan { Eligible { plan_id: String, expires_at: String, summary: String, destination: Option<String> }, Blocked { reason: String, error: Option<AppError> } }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState { Queued, Running, Succeeded, Failed, Skipped, Canceled }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult { pub operation_id: String, pub batch_id: Option<String>, pub member_id: Option<String>, pub repository_id: Option<String>, pub action: String, pub state: JobState, pub started_at: Option<String>, pub finished_at: Option<String>, pub exit_code: Option<i32>, pub reason: Option<String>, pub error: Option<AppError>, pub output_truncated: bool, pub outcome_uncertain: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMember { pub path: String, #[serde(skip_serializing_if = "Option::is_none")] pub label: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectContent { pub version: u32, pub name: String, pub repositories: Vec<ProjectMember> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project { pub project_id: String, pub revision: String, pub display_path: String, pub name: String, pub content: ProjectContent, pub members: Vec<RepositoryRow>, pub stale: bool, pub error: Option<AppError> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchRequest { pub project_id: String, pub revision: String, pub member_ids: Vec<String>, pub action: String, pub branch_filter: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCandidate { pub member_id: String, pub repository_id: Option<String>, pub label: String, pub eligible: bool, pub reason: Option<String>, pub destination: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPlan { pub plan_id: String, pub expires_at: String, pub candidates: Vec<BatchCandidate>, pub action: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch { pub batch_id: String, pub operation_ids: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest { pub request_id: String, pub kind: String, pub path: Option<String>, pub error: Option<AppError> }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_renderer_fixtures_deserialize() {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../src/test/fixtures/wire.json")).unwrap();
        for name in ["clean", "mixed", "detached", "unborn", "conflict", "missingUpstream", "unknownCounts"] {
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
