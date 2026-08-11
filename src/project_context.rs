use crate::domain::Project;
use std::collections::{HashMap, HashSet};

pub fn project_path(projects: &[Project], project_id: &str) -> Vec<Project> {
    let by_id = projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect::<HashMap<_, _>>();
    let mut path = Vec::new();
    let mut seen = HashSet::new();
    let mut current = by_id.get(project_id).copied();
    while let Some(project) = current {
        if !seen.insert(project.id.as_str()) {
            break;
        }
        path.push(project.clone());
        current = project
            .parent_id
            .as_deref()
            .and_then(|parent_id| by_id.get(parent_id).copied());
    }
    path.reverse();
    path
}
