use super::work_stealing::TaskClaim;
use std::path::{Path, PathBuf};

pub fn resolve_claim_workspace(
    repository_root: impl AsRef<Path>,
    worktree_root: impl AsRef<Path>,
    claim: &TaskClaim,
) -> Result<PathBuf, String> {
    let repository_root = std::fs::canonicalize(repository_root.as_ref())
        .map_err(|error| format!("repository root is unavailable: {error}"))?;
    let worktree_root = repository_root.join(worktree_root.as_ref());
    let name = format!("{}-{}", claim.owner, claim.task.id);
    if !valid_component(&name) {
        return Err("derived worktree name is unsafe".to_string());
    }
    let expected = worktree_root.join(name);
    let actual = std::fs::canonicalize(&expected)
        .map_err(|error| format!("claimed worktree is unavailable: {error}"))?;
    if !actual.starts_with(&repository_root) || actual != expected {
        return Err("claimed worktree escapes repository root".to_string());
    }
    Ok(actual)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations::migrate;
    use crate::tasks::model::NewTask;
    use crate::tasks::repository;
    use crate::tasks::work_stealing;
    use chrono::{Duration, TimeZone, Utc};
    use rusqlite::Connection;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn resolves_only_the_system_derived_claim_directory() {
        let root = std::env::temp_dir().join(format!("bloomery-workspace-{}", Uuid::new_v4()));
        let worktree_root = root.join(".agent").join("worktrees");
        fs::create_dir_all(&worktree_root).unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        let task = repository::create(
            &connection,
            NewTask {
                workspace_id: "local".to_string(),
                kind: "shared".to_string(),
                payload_json: "{}".to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap();
        let claim = worktree_root.join(format!("alice-{}", task.id));
        fs::create_dir_all(&claim).unwrap();
        let task_claim =
            work_stealing::claim_next(&mut connection, "local", "alice", now, Duration::minutes(1))
                .unwrap()
                .unwrap();
        assert_eq!(
            resolve_claim_workspace(&root, ".agent/worktrees", &task_claim).unwrap(),
            fs::canonicalize(claim).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
