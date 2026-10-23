use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_TAG_NAME_BYTES: usize = 255;
const MAX_TAG_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct TagPlan {
    commit_snapshot_id: String,
    preview: TagPreview,
}

impl TagPlan {
    pub(crate) fn preview(&self) -> TagPreview {
        self.preview.clone()
    }
}

impl GitEngine {
    pub(super) async fn prepare_tag(
        &self,
        repo: &Repository,
        status: &RepositoryStatus,
        commit_snapshot_id: &str,
        target_oid: &str,
        name: &str,
        message: &str,
    ) -> AppResult<TagPlan> {
        require_idle(status)?;
        validate_message(message)?;
        let full_ref = validate_name(&self.runner, repo, name).await?;
        require_missing_ref(&self.runner, repo, &full_ref).await?;
        require_identity(&self.runner, repo).await?;

        let snapshot = self.snapshot(&repo.id, commit_snapshot_id).await?;
        let snapshot_oid = snapshot.commit.as_ref().map(|commit| commit.0.as_str());
        if snapshot_oid != Some(target_oid) {
            return Err(AppError::new(
                "precondition_changed",
                "The selected commit changed while preparing the tag. Select it again.",
            ));
        }
        let resolved = self.resolve_commit(repo, target_oid).await?;
        if resolved != target_oid {
            return Err(AppError::new(
                "invalid_target",
                "The tag target must be the exact selected commit.",
            ));
        }
        let commit = history::commits(&self.runner, repo, std::slice::from_ref(&resolved))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                AppError::new("missing_object", "The selected commit is unavailable.")
            })?;
        let signing = if configured_bool(&self.runner, repo, "tag.gpgSign").await?
            || configured_bool(&self.runner, repo, "tag.forceSignAnnotated").await?
        {
            "configured"
        } else {
            "unsigned"
        };

        Ok(TagPlan {
            commit_snapshot_id: commit_snapshot_id.to_owned(),
            preview: TagPreview {
                name: name.to_owned(),
                full_ref,
                target_oid: resolved,
                subject: commit.message.lines().next().unwrap_or_default().to_owned(),
                annotated: true,
                signing: signing.to_owned(),
            },
        })
    }

    pub(super) async fn execute_tag(
        &self,
        plan: &PreparedAction,
        tag: &TagPlan,
        canceled: &AtomicBool,
    ) -> AppResult<ActionOutcome> {
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Operation was canceled before Git started.",
            ));
        }
        let status_before = self.observe(&plan.repository).await?;
        require_idle(status_before.status.as_ref().unwrap())?;
        let snapshot = self
            .snapshot(&plan.repository.id, &tag.commit_snapshot_id)
            .await?;
        if snapshot.commit.as_ref().map(|commit| commit.0.as_str())
            != Some(tag.preview.target_oid.as_str())
            || self
                .resolve_commit(&plan.repository, &tag.preview.target_oid)
                .await?
                != tag.preview.target_oid
        {
            return Err(AppError::new(
                "precondition_changed",
                "The selected commit changed after preview. Select it and preview the tag again.",
            ));
        }
        require_missing_ref(&self.runner, &plan.repository, &tag.preview.full_ref).await?;
        if canceled.load(Ordering::Acquire) {
            return Err(AppError::new(
                "canceled",
                "Operation was canceled before Git started.",
            ));
        }

        let output = match self
            .runner
            .run(
                &plan.repository.root,
                &plan.args,
                plan.input.clone(),
                true,
                runner::DIAGNOSTIC_LIMIT,
            )
            .await
        {
            Ok(output) => output,
            Err(error) => {
                if error.code == "outcome_uncertain"
                    && ref_exists(&self.runner, &plan.repository, &tag.preview.full_ref).await?
                {
                    return Err(AppError::new(
                        "outcome_uncertain",
                        "Git did not return a normal result, and the tag ref now exists. Refresh and inspect the tag before retrying.",
                    ));
                }
                return Err(error);
            }
        };
        if output.code != 0 {
            if ref_exists(&self.runner, &plan.repository, &tag.preview.full_ref).await? {
                let mut error = AppError::new(
                    "outcome_uncertain",
                    "Git reported an error, but the tag ref now exists. Refresh and inspect the tag before retrying.",
                );
                error.exit_code = Some(output.code);
                error.output_truncated = Some(output.truncated);
                error.details = Some(runner::redact(&String::from_utf8_lossy(&output.stderr)));
                return Err(error);
            }
            return runner::checked(output).map(|_| unreachable!());
        }

        let object_type = text(
            &self
                .runner
                .read(
                    &plan.repository.root,
                    &["cat-file", "-t", &tag.preview.full_ref],
                )
                .await
                .map_err(|error| verification_error("read the created tag object", error))?,
        );
        let peeled = self
            .resolve_commit(&plan.repository, &tag.preview.full_ref)
            .await
            .map_err(|error| verification_error("resolve the created tag target", error))?;
        let status_after = self
            .observe(&plan.repository)
            .await
            .map_err(|error| verification_error("verify repository state", error))?;
        if object_type != "tag"
            || peeled != tag.preview.target_oid
            || status_after.fingerprint != status_before.fingerprint
        {
            return Err(AppError::new(
                "outcome_uncertain",
                "The tag command completed, but the resulting tag or repository state differs from the reviewed result. Refresh and inspect before retrying.",
            ));
        }
        Ok(ActionOutcome {
            exit_code: output.code,
            output_truncated: output.truncated,
        })
    }
}

fn require_idle(status: &RepositoryStatus) -> AppResult<()> {
    if status.git_state != GitState::Idle || status.active_operation_id.is_some() {
        return Err(AppError::new(
            "operation_in_progress",
            "Finish the ongoing Git operation before creating a tag.",
        ));
    }
    Ok(())
}

fn validate_message(message: &str) -> AppResult<()> {
    if message.trim().is_empty() {
        return Err(AppError::new(
            "invalid_tag_message",
            "Enter a nonblank tag annotation.",
        ));
    }
    if message.len() > MAX_TAG_MESSAGE_BYTES || message.contains('\0') {
        return Err(AppError::new(
            "invalid_tag_message",
            "The tag annotation must be at most 64 KiB and cannot contain a NUL byte.",
        ));
    }
    Ok(())
}

async fn validate_name(
    runner: &runner::Runner,
    repo: &Repository,
    name: &str,
) -> AppResult<String> {
    if name.is_empty()
        || name.len() > MAX_TAG_NAME_BYTES
        || name.starts_with('-')
        || name.starts_with("refs/")
        || name.contains(['\0', '\r', '\n'])
    {
        return Err(invalid_name());
    }
    let full_ref = format!("refs/tags/{name}");
    let output = runner
        .run(
            &repo.root,
            &runner::args(&["check-ref-format", &full_ref]),
            None,
            false,
            runner::STRUCTURED_LIMIT,
        )
        .await?;
    if output.code != 0 {
        if output.code != 1 {
            runner::checked(output)?;
        }
        return Err(invalid_name());
    }
    Ok(full_ref)
}

fn invalid_name() -> AppError {
    AppError::new(
        "invalid_tag_name",
        "Enter a valid short tag name. Nested names are allowed; refs/tags/ prefixes and replacement flags are not.",
    )
}

async fn ref_exists(runner: &runner::Runner, repo: &Repository, full_ref: &str) -> AppResult<bool> {
    let output = runner
        .run(
            &repo.root,
            &runner::args(&["show-ref", "--verify", "--quiet", "--", full_ref]),
            None,
            false,
            runner::STRUCTURED_LIMIT,
        )
        .await?;
    match output.code {
        0 => Ok(true),
        1 => Ok(false),
        _ => runner::checked(output).map(|_| false),
    }
}

async fn require_missing_ref(
    runner: &runner::Runner,
    repo: &Repository,
    full_ref: &str,
) -> AppResult<()> {
    if ref_exists(runner, repo, full_ref).await? {
        return Err(AppError::new(
            "tag_exists",
            "A tag with this name already exists. Choose a different name; existing tags are never replaced.",
        ));
    }
    Ok(())
}

async fn require_identity(runner: &runner::Runner, repo: &Repository) -> AppResult<()> {
    let output = runner
        .run(
            &repo.root,
            &runner::args(&["var", "GIT_COMMITTER_IDENT"]),
            None,
            false,
            runner::STRUCTURED_LIMIT,
        )
        .await?;
    if output.code != 0 {
        return Err(AppError::new(
            "missing_identity",
            "Configure user.name and user.email before creating an annotated tag.",
        ));
    }
    Ok(())
}

async fn configured_bool(runner: &runner::Runner, repo: &Repository, key: &str) -> AppResult<bool> {
    let output = runner
        .run(
            &repo.root,
            &runner::args(&["config", "--bool", "--get", key]),
            None,
            false,
            runner::STRUCTURED_LIMIT,
        )
        .await?;
    match output.code {
        0 => Ok(text(&output.stdout) == "true"),
        1 => Ok(false),
        _ => runner::checked(output).map(|_| false),
    }
}

fn verification_error(action: &str, error: AppError) -> AppError {
    AppError::new(
        "outcome_uncertain",
        format!(
            "The tag command completed, but Git View could not {action}. Refresh and inspect the tag before retrying. {}",
            error.message
        ),
    )
}
