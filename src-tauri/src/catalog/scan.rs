use super::*;
use std::time::Duration;
use tokio::task::JoinSet;
use uuid::Uuid;

#[cfg(test)]
mod tests;

pub(super) fn placeholders(path: &Path, id: &str, content: &ProjectContent) -> Vec<RepositoryRow> {
    content
        .repositories
        .iter()
        .enumerate()
        .map(|(index, member)| RepositoryRow::Loading {
            member_id: hash(format!("{id}:{index}:{}", member.path).as_bytes()),
            label: member.label.clone().unwrap_or_else(|| {
                Path::new(&member.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }),
            display_path: if foreign_path(&member.path) {
                member.path.clone()
            } else {
                path.parent()
                    .unwrap()
                    .join(&member.path)
                    .to_string_lossy()
                    .into_owned()
            },
        })
        .collect()
}

impl Catalog {
    #[cfg(feature = "native-smoke")]
    pub(crate) async fn smoke_status_gate(&self, path: &Path, blocked: bool) {
        let key = path.to_string_lossy().into_owned();
        let mut gates = self.read_gates.lock().await;
        if blocked {
            gates.insert(key, Arc::new(Semaphore::new(0)));
        } else if let Some(gate) = gates.remove(&key) {
            gate.add_permits(4);
        }
    }
    // The project lock serializes publication, cancellation and replacement. No Git
    // process runs while it is held. Tokens also protect same-revision refreshes.
    pub(super) async fn start_scan(
        &self,
        engine: &GitEngine,
        mut project: Project,
        expected: Option<Option<String>>,
    ) -> AppResult<Project> {
        let mut projects = self.projects.lock().await;
        if let Some(expected) = expected {
            if let Some(current) = projects.get(&project.project_id) {
                if current.revision != project.revision
                    || current.status_scan.as_ref().map(|s| s.scan_id.clone()) != expected
                {
                    return Ok(current.clone());
                }
            }
        }
        let mut scans = self.scans.lock().await;
        if let Some(previous) = projects
            .get(&project.project_id)
            .and_then(|p| p.status_scan.as_ref())
        {
            if let Some(task) = scans.remove(&previous.scan_id) {
                task.abort();
            }
        }
        let scan_id = Uuid::new_v4().to_string();
        let members = placeholders(
            Path::new(&project.display_path),
            &project.project_id,
            &project.content,
        );
        project.status_scan = Some(ProjectStatusScan {
            scan_id: scan_id.clone(),
            state: if members.is_empty() {
                "complete"
            } else {
                "running"
            }
            .into(),
            pending_member_ids: members
                .iter()
                .map(|row| match row {
                    RepositoryRow::Loading { member_id, .. } => member_id.clone(),
                    _ => unreachable!(),
                })
                .collect(),
            error: None,
        });
        projects.insert(project.project_id.clone(), project.clone());
        if !members.is_empty() {
            let catalog = self.clone();
            let engine = engine.clone();
            let snapshot = project.clone();
            let task = tokio::spawn(async move {
                catalog.scan_rows(engine, snapshot, members).await;
            });
            scans.insert(scan_id, task.abort_handle());
        }
        Ok(project)
    }

    async fn scan_rows(&self, engine: GitEngine, snapshot: Project, members: Vec<RepositoryRow>) {
        let scan_id = &snapshot.status_scan.as_ref().unwrap().scan_id;
        let mut queue = members.into_iter().enumerate();
        let mut tasks = JoinSet::new();
        loop {
            // At most four tasks per scan, and four active reads across the catalog.
            while tasks.len() < 4 {
                let Some((
                    index,
                    RepositoryRow::Loading {
                        member_id,
                        label,
                        display_path,
                    },
                )) = queue.next()
                else {
                    break;
                };
                let engine = engine.clone();
                let permits = self.read_permits.clone();
                let foreign = foreign_path(&snapshot.content.repositories[index].path);
                #[cfg(any(test, feature = "native-smoke"))]
                let gate = self.read_gates.lock().await.get(&display_path).cloned();
                tasks.spawn(async move {
                    let _permit = permits
                        .acquire()
                        .await
                        .expect("catalog read permits remain open");
                    #[cfg(any(test, feature = "native-smoke"))]
                    let _gate = if let Some(gate) = gate {
                        Some(gate.acquire_owned().await.unwrap())
                    } else {
                        None
                    };
                    let result = if foreign {
                        Err(error("Path belongs to another operating system."))
                    } else {
                        engine.open_repository(Path::new(&display_path)).await
                    };
                    let row = match result {
                        Ok(opened) => RepositoryRow::Ready {
                            member_id: member_id.clone(),
                            repository_id: opened.repository_id,
                            label,
                            display_path,
                            status: opened.status,
                        },
                        Err(error) => RepositoryRow::Unavailable {
                            member_id: member_id.clone(),
                            label,
                            display_path,
                            error,
                        },
                    };
                    (index, member_id, row)
                });
            }
            let Some(result) = tasks.join_next().await else {
                break;
            };
            let mut projects = self.projects.lock().await;
            let Some(current) = projects.get_mut(&snapshot.project_id) else {
                break;
            };
            let Some(scan) = current.status_scan.as_mut() else {
                break;
            };
            if scan.scan_id != *scan_id || scan.state != "running" {
                break;
            }
            match result {
                Ok((index, member_id, row)) => {
                    current.members[index] = row;
                    scan.pending_member_ids.retain(|id| id != &member_id);
                    if scan.pending_member_ids.is_empty() {
                        scan.state = "complete".into();
                    }
                }
                Err(_) => {
                    scan.state = "failed".into();
                    scan.error =
                        Some("A status reader stopped unexpectedly. Refresh to retry.".into());
                    break;
                }
            }
        }
        self.scans.lock().await.remove(scan_id);
    }

    pub async fn cancel_scan(&self, project_id: &str, scan_id: &str) -> AppResult<Project> {
        let mut projects = self.projects.lock().await;
        let project = projects
            .get_mut(project_id)
            .ok_or_else(|| error("Open the project again."))?;
        if let Some(scan) = project.status_scan.as_mut() {
            if scan.scan_id == scan_id && scan.state == "running" {
                scan.state = "canceled".into();
                if let Some(task) = self.scans.lock().await.remove(scan_id) {
                    task.abort();
                }
            }
        }
        Ok(project.clone())
    }

    pub(super) async fn wait_for_scan(&self, project: Project) -> AppResult<Project> {
        loop {
            let current = self.get(&project.project_id).await?;
            if current
                .status_scan
                .as_ref()
                .is_none_or(|s| s.state != "running")
            {
                return Ok(current);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
