use crate::{
    catalog::Catalog,
    contracts::*,
    git::{GitEngine, PreparedAction},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::Emitter;
use tokio::sync::{Mutex, Semaphore};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn expires() -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
}
fn stale() -> AppError {
    AppError::new("expired_plan", "Preview expired. Preview the action again.")
}
struct ActionPlan {
    prepared: PreparedAction,
    repository_id: String,
    action: String,
    created: Instant,
    operation_id: Option<String>,
}
struct BatchEntry {
    candidate: BatchCandidate,
    prepared: Option<PreparedAction>,
    alias: Option<usize>,
}
struct HeldBatch {
    wire: BatchPlan,
    project_id: String,
    revision: String,
    entries: Vec<BatchEntry>,
    created: Instant,
    batch: Option<Batch>,
}
#[derive(Default)]
struct State {
    plans: HashMap<String, ActionPlan>,
    batches: HashMap<String, HeldBatch>,
    operations: HashMap<String, OperationResult>,
    order: VecDeque<String>,
    cancellations: HashMap<String, Arc<AtomicBool>>,
    sequence: u64,
}
#[derive(Clone)]
pub struct Operations {
    engine: GitEngine,
    catalog: Catalog,
    state: Arc<Mutex<State>>,
    jobs: Arc<Semaphore>,
    app: Arc<std::sync::Mutex<Option<tauri::AppHandle>>>,
}
impl Operations {
    pub fn new(engine: GitEngine, catalog: Catalog) -> Self {
        Self {
            engine,
            catalog,
            state: Arc::new(Mutex::new(State::default())),
            jobs: Arc::new(Semaphore::new(3)),
            app: Arc::new(std::sync::Mutex::new(None)),
        }
    }
    pub fn set_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }
    pub async fn get(&self, batch_id: Option<&str>) -> Vec<OperationResult> {
        let state = self.state.lock().await;
        state
            .order
            .iter()
            .filter_map(|id| state.operations.get(id))
            .filter(|op| batch_id.is_none_or(|id| op.batch_id.as_deref() == Some(id)))
            .cloned()
            .collect()
    }
    async fn store(&self, operation: OperationResult) {
        let mut state = self.state.lock().await;
        if !state.operations.contains_key(&operation.operation_id) {
            state.order.push_back(operation.operation_id.clone());
        }
        state
            .operations
            .insert(operation.operation_id.clone(), operation.clone());
        while state.order.len() > 2000 {
            let terminal = state.order.iter().position(|id| {
                state
                    .operations
                    .get(id)
                    .is_some_and(|op| !matches!(op.state, JobState::Queued | JobState::Running))
            });
            if let Some(index) = terminal {
                let key = state.order.remove(index).unwrap();
                state.operations.remove(&key);
            } else {
                break;
            }
        }
        state.sequence += 1;
        let sequence = state.sequence;
        drop(state);
        if let Some(app) = self.app.lock().unwrap().as_ref() {
            let _ = app.emit(
                "operation-progress",
                serde_json::json!({"id":id(), "sequence":sequence,"operation":operation}),
            );
        }
    }
    pub async fn plan_action(
        &self,
        repository_id: &str,
        action: RepositoryAction,
    ) -> RepositoryActionPlan {
        let kind = action.kind().to_owned();
        match self.engine.prepare_action(repository_id, action).await {
            Err(error) => RepositoryActionPlan::Blocked {
                reason: error.message.clone(),
                error: Some(error),
            },
            Ok(prepared) => {
                let plan_id = id();
                let summary = prepared.summary.clone();
                let destination = prepared.destination.clone();
                let rebase_preview = prepared.rebase.as_ref().and_then(|plan| plan.preview());
                let mut state = self.state.lock().await;
                state.plans.retain(|_, p| {
                    p.created.elapsed() < Duration::from_secs(300) || p.operation_id.is_some()
                });
                if state.plans.len() > 2000 {
                    state
                        .plans
                        .retain(|_, p| p.created.elapsed() < Duration::from_secs(300));
                }
                state.plans.insert(
                    plan_id.clone(),
                    ActionPlan {
                        prepared,
                        repository_id: repository_id.into(),
                        action: kind,
                        created: Instant::now(),
                        operation_id: None,
                    },
                );
                RepositoryActionPlan::Eligible {
                    plan_id,
                    expires_at: expires(),
                    summary,
                    destination,
                    rebase_preview,
                }
            }
        }
    }
    pub async fn run_action(&self, plan_id: &str) -> AppResult<OperationResult> {
        let mut state = self.state.lock().await;
        let plan = state.plans.get(plan_id).ok_or_else(stale)?;
        if let Some(op) = &plan.operation_id {
            return state.operations.get(op).cloned().ok_or_else(stale);
        }
        if plan.created.elapsed() > Duration::from_secs(300) {
            return Err(stale());
        }
        let prepared = plan.prepared.clone();
        let operation = queued(None, None, Some(plan.repository_id.clone()), &plan.action);
        state.plans.get_mut(plan_id).unwrap().operation_id = Some(operation.operation_id.clone());
        state.order.push_back(operation.operation_id.clone());
        state
            .operations
            .insert(operation.operation_id.clone(), operation.clone());
        drop(state);
        let service = self.clone();
        let task_op = operation.clone();
        tokio::spawn(async move {
            service
                .execute(prepared, task_op, Arc::new(AtomicBool::new(false)))
                .await;
        });
        Ok(operation)
    }
    async fn execute(
        &self,
        prepared: PreparedAction,
        mut operation: OperationResult,
        cancel: Arc<AtomicBool>,
    ) -> OperationResult {
        let _permit = self.jobs.acquire().await.unwrap();
        if cancel.load(Ordering::SeqCst) {
            operation.state = JobState::Canceled;
            operation.reason = Some("Canceled before starting.".into());
            operation.finished_at = Some(now());
            self.store(operation.clone()).await;
            return operation;
        }
        operation.state = JobState::Running;
        operation.started_at = Some(now());
        self.store(operation.clone()).await;
        match self
            .engine
            .execute_cancelable(&prepared, cancel.clone())
            .await
        {
            Ok(outcome) => {
                operation.state = JobState::Succeeded;
                operation.exit_code = Some(outcome.exit_code);
                operation.output_truncated = outcome.output_truncated;
            }
            Err(error) => {
                operation.state = if error.code == "rebase_stopped" {
                    JobState::Stopped
                } else if error.code == "canceled" {
                    JobState::Canceled
                } else if error.code == "precondition_changed" || error.code == "stale_snapshot" {
                    JobState::Skipped
                } else {
                    JobState::Failed
                };
                operation.outcome_uncertain = error.code == "outcome_uncertain"
                    || error.code.contains("timeout")
                    || error.code.contains("interrupted");
                operation.exit_code = error.exit_code;
                operation.output_truncated = error.output_truncated.unwrap_or(false);
                operation.reason = Some(error.message.clone());
                operation.error = Some(error);
            }
        }
        if cancel.load(Ordering::SeqCst) && operation.state == JobState::Succeeded {
            operation.reason =
                Some("Cancellation was requested while running; the action completed.".into());
        }
        if let Some(repo) = &operation.repository_id {
            let _ = self.engine.get_status(repo).await;
        }
        operation.finished_at = Some(now());
        self.store(operation.clone()).await;
        operation
    }
    pub async fn plan_batch(&self, request: BatchRequest) -> AppResult<BatchPlan> {
        if !matches!(
            request.action.as_str(),
            "fetch" | "pull" | "push" | "switchBranch"
        ) {
            return Err(AppError::new(
                "invalid_action",
                "Batch supports fetch, pull, configured-upstream push or switching to an existing local branch.",
            ));
        }
        if request.action == "switchBranch" {
            let name = request.target_branch.as_deref().unwrap_or_default();
            if name.is_empty()
                || name.len() > 1024
                || name.starts_with('-')
                || name.starts_with("refs/")
                || name.contains("@{")
                || name.chars().any(|c| c.is_whitespace() || c.is_control())
            {
                return Err(AppError::new(
                    "invalid_branch",
                    "Enter one literal local branch name, without refs/heads/ or revision syntax.",
                ));
            }
        } else if request.target_branch.is_some() {
            return Err(AppError::new(
                "invalid_action",
                "A target branch is only valid for a switch batch.",
            ));
        }
        let project = self
            .catalog
            .verify_revision(&request.project_id, &request.revision)
            .await?;
        if project
            .status_scan
            .as_ref()
            .is_some_and(|scan| !scan.pending_member_ids.is_empty())
        {
            return Err(AppError::new(
                "status_incomplete",
                "Finish refreshing group status before previewing a batch.",
            ));
        }
        if request.member_ids.len() > 1000 {
            return Err(AppError::new(
                "invalid_selection",
                "Selection exceeds project limit.",
            ));
        }
        let mut selected = HashSet::new();
        let mut aliases = HashMap::new();
        let mut entries = Vec::new();
        for member_id in &request.member_ids {
            if !selected.insert(member_id.clone()) {
                continue;
            }
            let row = project
                .members
                .iter()
                .find(|row| match row {
                    RepositoryRow::Ready { member_id: id, .. }
                    | RepositoryRow::Loading { member_id: id, .. }
                    | RepositoryRow::Unavailable { member_id: id, .. } => id == member_id,
                })
                .ok_or_else(|| {
                    AppError::new(
                        "invalid_selection",
                        "Selected member is no longer in the project.",
                    )
                })?;
            let (repository_id, label) = match row {
                RepositoryRow::Ready {
                    repository_id,
                    label,
                    ..
                } => (Some(repository_id.clone()), label.clone()),
                RepositoryRow::Loading { label, .. } | RepositoryRow::Unavailable { label, .. } => {
                    (None, label.clone())
                }
            };
            let mut candidate = BatchCandidate {
                member_id: member_id.clone(),
                repository_id: repository_id.clone(),
                label,
                eligible: false,
                reason: None,
                destination: None,
                summary: None,
            };
            let mut prepared = None;
            let mut alias = None;
            if let Some(repository_id) = repository_id {
                let outcome: AppResult<PreparedAction> = async {
                    let status = self.engine.get_status(&repository_id).await?;
                    if let Some(branch) = &request.branch_filter {
                        if !matches!(&status.head, Head::Branch {name,..}|Head::Unborn {name} if name==branch) { return Err(AppError::new("branch_filter", "Current branch does not match the exact branch filter.")); }
                    }
                    if request.action == "push" {
                        return self.engine.prepare_upstream_push(&repository_id, &status).await;
                    }
                    let action = if request.action == "switchBranch" {
                        let target = request.target_branch.as_ref().unwrap();
                        match &status.head {
                            Head::Branch { name, .. } if name == target => return Err(AppError::new("already_on_branch", "Already on the target branch; no switch is needed.")),
                            Head::Branch { .. } => {},
                            _ => return Err(AppError::new("invalid_head", "Group switching requires an attached, existing source branch. Inspect this repository individually.")),
                        }
                        RepositoryAction::SwitchBranch { status_snapshot_id: status.snapshot_id, local_ref: format!("refs/heads/{target}") }
                    } else if request.action == "pull" { RepositoryAction::Pull { status_snapshot_id:status.snapshot_id } } else {
                        let remotes = self.engine.list_remotes(&repository_id).await?;
                        let remote = remotes.iter().find(|r|r.default_fetch).ok_or_else(||AppError::new("no_remote", "Choose a remote in the repository screen."))?;
                        RepositoryAction::Fetch {remote_id:remote.id.clone()}
                    };
                    let mut prepared = self.engine.prepare_action(&repository_id,action).await?;
                    if request.action == "fetch" && request.branch_filter.is_some() { prepared.guard_head(status.head); }
                    Ok(prepared)
                }.await;
                match outcome {
                    Ok(plan) => {
                        candidate.eligible = true;
                        candidate.destination = plan.destination.clone();
                        if matches!(request.action.as_str(), "switchBranch" | "push") {
                            candidate.summary = Some(plan.summary.clone());
                        }
                        let key = plan.fetch_coalesce_key().unwrap_or(repository_id);
                        alias = aliases.get(&key).copied();
                        if alias.is_none() {
                            aliases.insert(key, entries.len());
                        }
                        prepared = Some(plan);
                    }
                    Err(e) => candidate.reason = Some(e.message),
                }
            } else {
                candidate.reason = Some(match row {
                    RepositoryRow::Unavailable { error, .. } => error.message.clone(),
                    _ => "Repository is unavailable.".into(),
                });
            }
            entries.push(BatchEntry {
                candidate,
                prepared,
                alias,
            });
        }
        let wire = BatchPlan {
            plan_id: id(),
            expires_at: expires(),
            candidates: entries.iter().map(|e| e.candidate.clone()).collect(),
            action: request.action,
            target_branch: request.target_branch,
        };
        let mut state = self.state.lock().await;
        state
            .batches
            .retain(|_, p| p.created.elapsed() < Duration::from_secs(300) || p.batch.is_some());
        if state.batches.len() > 1000 {
            state
                .batches
                .retain(|_, p| p.created.elapsed() < Duration::from_secs(300));
        }
        state.batches.insert(
            wire.plan_id.clone(),
            HeldBatch {
                wire: wire.clone(),
                project_id: request.project_id,
                revision: request.revision,
                entries,
                created: Instant::now(),
                batch: None,
            },
        );
        Ok(wire)
    }
    pub async fn run_batch(&self, plan_id: &str) -> AppResult<Batch> {
        let (project_id, revision) = {
            let state = self.state.lock().await;
            let plan = state.batches.get(plan_id).ok_or_else(stale)?;
            if let Some(batch) = &plan.batch {
                return Ok(batch.clone());
            }
            if plan.created.elapsed() > Duration::from_secs(300) {
                return Err(stale());
            }
            (plan.project_id.clone(), plan.revision.clone())
        };
        self.catalog.verify_revision(&project_id, &revision).await?;
        let mut state = self.state.lock().await;
        let plan = state.batches.get_mut(plan_id).ok_or_else(stale)?;
        if let Some(batch) = &plan.batch {
            return Ok(batch.clone());
        }
        let batch_id = id();
        let mut operations = Vec::new();
        let mut jobs = Vec::new();
        for entry in &plan.entries {
            let mut operation = queued(
                Some(batch_id.clone()),
                Some(entry.candidate.member_id.clone()),
                entry.candidate.repository_id.clone(),
                &plan.wire.action,
            );
            operation.target_branch = plan.wire.target_branch.clone();
            if plan.wire.action == "push" {
                operation.destination = entry.candidate.destination.clone();
                operation.summary = entry.candidate.summary.clone();
            }
            if !entry.candidate.eligible {
                operation.state = JobState::Skipped;
                operation.reason = entry.candidate.reason.clone();
                operation.finished_at = Some(now());
            }
            operations.push(operation.clone());
            jobs.push((entry.prepared.clone(), entry.alias, operation));
        }
        let batch = Batch {
            batch_id: batch_id.clone(),
            operation_ids: operations.iter().map(|o| o.operation_id.clone()).collect(),
        };
        plan.batch = Some(batch.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancellations.insert(batch_id, cancel.clone());
        for op in operations {
            state.order.push_back(op.operation_id.clone());
            state.operations.insert(op.operation_id.clone(), op);
        }
        drop(state);
        let service = self.clone();
        tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            let mut alias_jobs = Vec::new();
            let mut outcomes = HashMap::new();
            for (index, (prepared, alias, operation)) in jobs.into_iter().enumerate() {
                if operation.state == JobState::Skipped {
                    outcomes.insert(index, operation);
                    continue;
                }
                if let Some(first) = alias {
                    alias_jobs.push((first, operation));
                    continue;
                }
                if let Some(prepared) = prepared {
                    let s = service.clone();
                    let c = cancel.clone();
                    tasks.spawn(async move { (index, s.execute(prepared, operation, c).await) });
                }
            }
            while let Some(result) = tasks.join_next().await {
                if let Ok((index, op)) = result {
                    outcomes.insert(index, op);
                }
            }
            for (first, mut alias) in alias_jobs {
                if let Some(source) = outcomes.get(&first) {
                    alias.state = source.state.clone();
                    alias.started_at = source.started_at.clone();
                    alias.finished_at = source.finished_at.clone();
                    alias.exit_code = source.exit_code;
                    alias.error = source.error.clone();
                    alias.output_truncated = source.output_truncated;
                    alias.outcome_uncertain = source.outcome_uncertain;
                    alias.reason = Some(format!(
                        "Same repository as another member. {}",
                        source.reason.as_deref().unwrap_or("Shared result.")
                    ));
                } else {
                    alias.state = JobState::Failed;
                    alias.finished_at = Some(now());
                    alias.reason =
                        Some("Operation worker interrupted; refresh repository state.".into());
                    alias.outcome_uncertain = true;
                }
                service.store(alias).await;
            }
        });
        Ok(batch)
    }
    pub async fn cancel(&self, batch_id: &str) -> AppResult<()> {
        let state = self.state.lock().await;
        let cancel = state
            .cancellations
            .get(batch_id)
            .ok_or_else(|| AppError::new("unknown_batch", "Batch is no longer retained."))?;
        cancel.store(true, Ordering::SeqCst);
        Ok(())
    }
}
fn queued(
    batch_id: Option<String>,
    member_id: Option<String>,
    repository_id: Option<String>,
    action: &str,
) -> OperationResult {
    OperationResult {
        operation_id: id(),
        batch_id,
        member_id,
        repository_id,
        action: action.into(),
        target_branch: None,
        destination: None,
        summary: None,
        state: JobState::Queued,
        started_at: None,
        finished_at: None,
        exit_code: None,
        reason: None,
        error: None,
        output_truncated: false,
        outcome_uncertain: false,
    }
}
