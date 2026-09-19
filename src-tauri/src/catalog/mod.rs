use crate::{contracts::*, git::GitEngine};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{Mutex, Semaphore};

const MAX_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Default)]
pub struct Catalog {
    projects: Arc<Mutex<HashMap<String, Project>>>,
    saves: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}
fn error(message: impl Into<String>) -> AppError {
    AppError::new("invalid_project", message)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn validate(content: &ProjectContent) -> AppResult<()> {
    if content.version != 1 {
        return Err(error("Unsupported project version; expected 1."));
    }
    if content.name.trim().is_empty() {
        return Err(error("name must be nonblank."));
    }
    if content.repositories.len() > 1000 {
        return Err(error("At most 1,000 repositories are supported."));
    }
    for (i, member) in content.repositories.iter().enumerate() {
        if member.path.trim().is_empty() || member.path.contains('\0') {
            return Err(error(format!(
                "repositories[{i}].path must be a nonblank local path."
            )));
        }
        if member.label.as_ref().is_some_and(|s| s.trim().is_empty()) {
            return Err(error(format!("repositories[{i}].label must be nonblank.")));
        }
    }
    Ok(())
}
pub fn read_project(path: &Path) -> AppResult<(ProjectContent, String)> {
    let file = std::fs::File::open(path).map_err(|e| error(format!("{}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(e.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(error("Project exceeds 1 MiB."));
    }
    let content: ProjectContent =
        serde_json::from_slice(&bytes).map_err(|e| error(format!("{}: {e}", path.display())))?;
    validate(&content)?;
    Ok((content, hash(&bytes)))
}
fn foreign_path(path: &str) -> bool {
    if cfg!(windows) {
        path.starts_with('/') && !path.starts_with("//")
    } else {
        path.as_bytes().get(1) == Some(&b':') || path.starts_with("\\\\")
    }
}
fn relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    let a: Vec<_> = base.components().collect();
    let b: Vec<_> = target.components().collect();
    if a.first() != b.first() {
        return None;
    }
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..a.len() {
        out.push("..");
    }
    for part in &b[common..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    Some(out)
}
fn portable(mut content: ProjectContent, file: &Path) -> ProjectContent {
    for member in &mut content.repositories {
        let p = Path::new(&member.path);
        if p.is_absolute() && !foreign_path(&member.path) {
            if let Some(relative) = relative_path(file.parent().unwrap(), p) {
                member.path = relative.to_string_lossy().into_owned();
                if cfg!(windows) {
                    member.path = member.path.replace('\\', "/");
                }
            }
        }
    }
    content
}
impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }
    async fn rows(
        engine: &GitEngine,
        path: &Path,
        project_id: &str,
        content: &ProjectContent,
    ) -> Vec<RepositoryRow> {
        let permits = Arc::new(Semaphore::new(4));
        let mut tasks = tokio::task::JoinSet::new();
        for (index, member) in content.repositories.iter().enumerate() {
            let path = path.parent().unwrap().join(&member.path);
            let foreign = foreign_path(&member.path);
            let display_path = if foreign {
                member.path.clone()
            } else {
                path.to_string_lossy().into_owned()
            };
            let label = member.label.clone().unwrap_or_else(|| {
                Path::new(&member.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let member_id = hash(format!("{project_id}:{index}:{}", member.path).as_bytes());
            let engine = engine.clone();
            let permits = permits.clone();
            tasks.spawn(async move {
                let _permit = permits.acquire().await.unwrap();
                let result = if foreign {
                    Err(error("Path belongs to another operating system."))
                } else {
                    engine.open_repository(&path).await
                };
                let row = match result {
                    Ok(opened) => RepositoryRow::Ready {
                        member_id,
                        repository_id: opened.repository_id,
                        label,
                        display_path,
                        status: opened.status,
                    },
                    Err(error) => RepositoryRow::Unavailable {
                        member_id,
                        label,
                        display_path,
                        error,
                    },
                };
                (index, row)
            });
        }
        let mut rows = Vec::new();
        while let Some(result) = tasks.join_next().await {
            rows.push(result.expect("status task panicked"));
        }
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, row)| row).collect()
    }
    pub async fn open(&self, engine: &GitEngine, path: &Path) -> AppResult<Project> {
        let path = std::fs::canonicalize(path).map_err(|e| error(e.to_string()))?;
        let (content, revision) = read_project(&path)?;
        let display_path = path
            .to_str()
            .ok_or_else(|| error("Project filename cannot be represented in JSON."))?
            .to_owned();
        let project_id = hash(display_path.as_bytes());
        let members = Self::rows(engine, &path, &project_id, &content).await;
        let project = Project {
            project_id: project_id.clone(),
            revision,
            display_path,
            name: content.name.clone(),
            content,
            members,
            stale: false,
            error: None,
        };
        self.projects
            .lock()
            .await
            .insert(project_id, project.clone());
        Ok(project)
    }
    pub async fn get(&self, id: &str) -> AppResult<Project> {
        self.projects
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| error("Open the project again."))
    }
    pub async fn refresh(&self, engine: &GitEngine, id: &str) -> AppResult<Project> {
        let mut project = self.get(id).await?;
        match read_project(Path::new(&project.display_path)) {
            Ok((_, revision)) if revision == project.revision => {
                project.stale = false;
                project.error = None;
            }
            Ok(_) => {
                project.stale = true;
                project.error = Some(AppError::new(
                    "project_changed",
                    "Project changed on disk. Reload before starting another batch.",
                ));
            }
            Err(e) => {
                project.stale = true;
                project.error = Some(e);
            }
        }
        project.members = Self::rows(
            engine,
            Path::new(&project.display_path),
            id,
            &project.content,
        )
        .await;
        let mut state = self.projects.lock().await;
        // A concurrent explicit reload/save wins over this older observation.
        if state
            .get(id)
            .is_some_and(|p| p.revision != project.revision)
        {
            return Ok(state[id].clone());
        }
        state.insert(id.to_owned(), project.clone());
        Ok(project)
    }
    pub async fn verify_revision(&self, id: &str, expected: &str) -> AppResult<Project> {
        let project = self.get(id).await?;
        if project.stale
            || project.revision != expected
            || read_project(Path::new(&project.display_path))?.1 != expected
        {
            return Err(AppError::new(
                "project_changed",
                "Project changed. Reload and preview again.",
            ));
        }
        Ok(project)
    }
    pub async fn create(
        &self,
        engine: &GitEngine,
        path: &Path,
        content: ProjectContent,
    ) -> AppResult<Project> {
        validate(&content)?;
        let parent = std::fs::canonicalize(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .map_err(|e| error(e.to_string()))?;
        let path = parent.join(
            path.file_name()
                .ok_or_else(|| error("Choose a filename."))?,
        );
        let content = portable(content, &path);
        let mut temp =
            tempfile::NamedTempFile::new_in(&parent).map_err(|e| error(e.to_string()))?;
        let bytes = serde_json::to_vec_pretty(&content).map_err(|e| error(e.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error("Project exceeds 1 MiB."));
        }
        temp.write_all(&bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| error(e.to_string()))?;
        temp.persist_noclobber(&path)
            .map_err(|e| AppError::new("project_exists", e.to_string()))?;
        self.open(engine, &path).await
    }
    pub async fn save(
        &self,
        engine: &GitEngine,
        id: &str,
        expected: &str,
        content: ProjectContent,
    ) -> AppResult<Project> {
        validate(&content)?;
        let save_lock = self
            .saves
            .lock()
            .await
            .entry(id.to_owned())
            .or_default()
            .clone();
        let _save_guard = save_lock.lock().await;
        let project = self.verify_revision(id, expected).await?;
        let path = Path::new(&project.display_path);
        let metadata = std::fs::metadata(path).map_err(|e| error(e.to_string()))?;
        if metadata.permissions().readonly() {
            return Err(AppError::new(
                "read_only",
                "Project is read-only. Use Save as.",
            ));
        }
        let content = portable(content, path);
        let bytes = serde_json::to_vec_pretty(&content).map_err(|e| error(e.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(error("Project exceeds 1 MiB."));
        }
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
            .map_err(|e| error(e.to_string()))?;
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|e| error(e.to_string()))?;
        temp.write_all(&bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| error(e.to_string()))?;
        self.verify_revision(id, expected).await?;
        temp.persist(path).map_err(|e| error(e.to_string()))?;
        self.open(engine, path).await
    }
}
