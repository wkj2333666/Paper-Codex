use crate::{
    db::Database,
    workspace::{atomic_write, Workspace},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectReadme {
    pub markdown: String,
    pub revision: String,
    pub updated_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectReadmeError {
    #[error("project does not exist")]
    ProjectNotFound,
    #[error("project slug is not a safe path component")]
    InvalidProjectSlug,
    #[error("project README changed since it was loaded")]
    Conflict { current_revision: String },
    #[error(transparent)]
    Database(#[from] anyhow::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct ProjectReadmeStore {
    db: Database,
    workspace: Workspace,
}

impl ProjectReadmeStore {
    pub fn new(db: Database, workspace: Workspace) -> Self {
        Self { db, workspace }
    }

    pub async fn read(&self, project_id: &str) -> Result<ProjectReadme, ProjectReadmeError> {
        let (path, initial) = self.resolve(project_id).await?;
        if tokio::fs::metadata(&path).await.is_ok() {
            return self.read_path(&path).await;
        }
        let _guard = self.workspace.lock_project_readme().await;
        self.read_or_initialize(&path, &initial).await
    }

    pub async fn write(
        &self,
        project_id: &str,
        markdown: &str,
        expected_revision: &str,
    ) -> Result<ProjectReadme, ProjectReadmeError> {
        let _guard = self.workspace.lock_project_readme().await;
        let (path, initial) = self.resolve(project_id).await?;
        let current = self.read_or_initialize(&path, &initial).await?;
        if current.revision != expected_revision {
            return Err(ProjectReadmeError::Conflict {
                current_revision: current.revision,
            });
        }
        atomic_write(&path, markdown.as_bytes()).await?;
        self.read_path(&path).await
    }

    async fn read_or_initialize(
        &self,
        path: &Path,
        initial: &str,
    ) -> Result<ProjectReadme, ProjectReadmeError> {
        if tokio::fs::metadata(path).await.is_err() {
            atomic_write(path, initial.as_bytes()).await?;
        }
        self.read_path(path).await
    }

    async fn resolve(
        &self,
        project_id: &str,
    ) -> Result<(std::path::PathBuf, String), ProjectReadmeError> {
        let project = self
            .db
            .get_project(project_id)
            .await?
            .ok_or(ProjectReadmeError::ProjectNotFound)?;
        if !single_normal_component(&project.slug) {
            return Err(ProjectReadmeError::InvalidProjectSlug);
        }
        let directory = self.workspace.root().join("projects").join(&project.slug);
        let legacy = directory.join("project.md");
        let initial = match tokio::fs::read_to_string(legacy).await {
            Ok(markdown) => markdown,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                format!("# {}\n\n{}\n", project.name, project.purpose)
            }
            Err(error) => return Err(error.into()),
        };
        Ok((directory.join("README.md"), initial))
    }

    async fn read_path(&self, path: &Path) -> Result<ProjectReadme, ProjectReadmeError> {
        let markdown = tokio::fs::read_to_string(path).await?;
        let metadata = tokio::fs::metadata(path).await?;
        let modified: DateTime<Utc> = metadata.modified()?.into();
        Ok(ProjectReadme {
            revision: hex::encode(Sha256::digest(markdown.as_bytes())),
            markdown,
            updated_at: modified.to_rfc3339(),
        })
    }
}

fn single_normal_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn first_read_waits_for_the_initialization_lock() {
        let root = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(root.path()).await.unwrap();
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let project_id = db
            .create_project("locked-initialization", "Locked", "")
            .await
            .unwrap();
        let store = Arc::new(ProjectReadmeStore::new(db, workspace.clone()));
        let guard = workspace.lock_project_readme().await;
        let mut read = tokio::spawn({
            let store = store.clone();
            async move { store.read(&project_id).await }
        });

        assert!(tokio::time::timeout(Duration::from_millis(100), &mut read)
            .await
            .is_err());
        drop(guard);
        assert!(read.await.unwrap().is_ok());
    }
}
