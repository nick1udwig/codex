#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use chrono::Utc;
use codex_core::EventPersistenceMode;
use codex_core::RolloutRecorder;
use codex_core::RolloutRecorderParams;
use codex_core::config::ConfigBuilder;
use codex_core::find_archived_thread_path_by_id_str;
use codex_core::find_thread_path_by_id_str;
use codex_core::find_thread_path_by_name_str;
use codex_core::find_thread_path_by_name_str_in_cwd;
use codex_core::project_sessions_root;
use codex_protocol::ThreadId;
use codex_protocol::models::BaseInstructions;
use codex_protocol::protocol::SessionSource;
use codex_state::StateRuntime;
use codex_state::ThreadMetadataBuilder;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use uuid::Uuid;

/// Create <subdir>/YYYY/MM/DD and write a minimal rollout file containing the
/// provided conversation id in the SessionMeta line. Returns the absolute path.
fn write_minimal_rollout_with_id_in_subdir(codex_home: &Path, subdir: &str, id: Uuid) -> PathBuf {
    let sessions = codex_home.join(subdir).join("2024/01/01");
    std::fs::create_dir_all(&sessions).unwrap();
    write_minimal_rollout_with_id_at(sessions.as_path(), id, Path::new("."))
}

fn write_minimal_rollout_with_id_at(dir: &Path, id: Uuid, cwd: &Path) -> PathBuf {
    let file = dir.join(format!("rollout-2024-01-01T00-00-00-{id}.jsonl"));
    let mut f = std::fs::File::create(&file).unwrap();
    // Minimal first line: session_meta with the id so content search can find it
    writeln!(
        f,
        "{}",
        serde_json::json!({
            "timestamp": "2024-01-01T00:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": id,
                "timestamp": "2024-01-01T00:00:00Z",
                "cwd": cwd,
                "originator": "test",
                "cli_version": "test",
                "model_provider": "test-provider"
            }
        })
    )
    .unwrap();

    file
}

/// Create sessions/YYYY/MM/DD and write a minimal rollout file containing the
/// provided conversation id in the SessionMeta line. Returns the absolute path.
fn write_minimal_rollout_with_id(codex_home: &Path, id: Uuid) -> PathBuf {
    write_minimal_rollout_with_id_in_subdir(codex_home, "sessions", id)
}

async fn upsert_thread_metadata(codex_home: &Path, thread_id: ThreadId, rollout_path: PathBuf) {
    let runtime = StateRuntime::init(codex_home.to_path_buf(), "test-provider".to_string(), None)
        .await
        .unwrap();
    runtime.mark_backfill_complete(None).await.unwrap();
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path,
        Utc::now(),
        SessionSource::default(),
    );
    builder.cwd = codex_home.to_path_buf();
    let metadata = builder.build("test-provider");
    runtime.upsert_thread(&metadata).await.unwrap();
}

#[tokio::test]
async fn find_locates_rollout_file_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found.unwrap(), expected);
}

#[tokio::test]
async fn find_handles_gitignore_covering_codex_home_directory() {
    let repo = TempDir::new().unwrap();
    let codex_home = repo.path().join(".codex");
    std::fs::create_dir_all(&codex_home).unwrap();
    std::fs::write(repo.path().join(".gitignore"), ".codex/**\n").unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(&codex_home, id);

    let found = find_thread_path_by_id_str(&codex_home, &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_prefers_sqlite_path_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let thread_id = ThreadId::from_string(&id.to_string()).unwrap();
    let db_path = home.path().join(format!(
        "sessions/2030/12/30/rollout-2030-12-30T00-00-00-{id}.jsonl"
    ));
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    std::fs::write(&db_path, "").unwrap();
    write_minimal_rollout_with_id(home.path(), id);
    upsert_thread_metadata(home.path(), thread_id, db_path.clone()).await;

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(db_path));
}

#[tokio::test]
async fn find_falls_back_to_filesystem_when_sqlite_has_no_match() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);
    let unrelated_id = Uuid::new_v4();
    let unrelated_thread_id = ThreadId::from_string(&unrelated_id.to_string()).unwrap();
    let unrelated_path = home
        .path()
        .join("sessions/2030/12/30/rollout-2030-12-30T00-00-00-unrelated.jsonl");
    upsert_thread_metadata(home.path(), unrelated_thread_id, unrelated_path).await;

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_ignores_granular_gitignore_rules() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);
    std::fs::write(home.path().join("sessions/.gitignore"), "*.jsonl\n").unwrap();

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_locates_rollout_file_written_by_recorder() -> std::io::Result<()> {
    // Ensures the name-based finder locates a rollout produced by the real recorder.
    let home = TempDir::new().unwrap();
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await?;
    let thread_id = ThreadId::new();
    let thread_name = "named thread";
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            thread_id,
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await?;
    recorder.persist().await?;
    recorder.flush().await?;

    let index_path = home.path().join("session_index.jsonl");
    std::fs::write(
        &index_path,
        format!(
            "{}\n",
            serde_json::json!({
                "id": thread_id,
                "thread_name": thread_name,
                "updated_at": "2024-01-01T00:00:00Z"
            })
        ),
    )?;

    let found = find_thread_path_by_name_str(home.path(), thread_name).await?;

    let path = found.expect("expected rollout path to be found");
    assert!(path.exists());
    let contents = std::fs::read_to_string(&path)?;
    assert!(contents.contains(&thread_id.to_string()));
    recorder.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn find_name_in_cwd_prefers_project_session_index() -> std::io::Result<()> {
    let home = TempDir::new().unwrap();
    let thread_name = "shared-name";

    let cwd_a = home.path().join("workspace-a");
    let cwd_b = home.path().join("workspace-b");
    std::fs::create_dir_all(&cwd_a)?;
    std::fs::create_dir_all(&cwd_b)?;

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    let dir_a = project_sessions_root(home.path(), cwd_a.as_path()).join("2024/01/01");
    let dir_b = project_sessions_root(home.path(), cwd_b.as_path()).join("2024/01/01");
    std::fs::create_dir_all(&dir_a)?;
    std::fs::create_dir_all(&dir_b)?;

    let rollout_a = write_minimal_rollout_with_id_at(dir_a.as_path(), id_a, cwd_a.as_path());
    let _rollout_b = write_minimal_rollout_with_id_at(dir_b.as_path(), id_b, cwd_b.as_path());

    // Global index newest entry points to workspace-b.
    let global_index = home.path().join("session_index.jsonl");
    std::fs::write(
        &global_index,
        format!(
            "{}\n{}\n",
            serde_json::json!({
                "id": id_a,
                "thread_name": thread_name,
                "updated_at": "2024-01-01T00:00:00Z"
            }),
            serde_json::json!({
                "id": id_b,
                "thread_name": thread_name,
                "updated_at": "2024-01-02T00:00:00Z"
            })
        ),
    )?;

    let project_index =
        project_sessions_root(home.path(), cwd_a.as_path()).join("session_index.jsonl");
    std::fs::write(
        &project_index,
        format!(
            "{}\n",
            serde_json::json!({
                "id": id_a,
                "thread_name": thread_name,
                "updated_at": "2024-01-03T00:00:00Z"
            })
        ),
    )?;

    let found = find_thread_path_by_name_str_in_cwd(home.path(), cwd_a.as_path(), thread_name)
        .await?
        .expect("expected project-scoped name lookup to find a rollout");
    assert_eq!(found, rollout_a);
    Ok(())
}

#[tokio::test]
async fn find_name_in_cwd_falls_back_to_older_global_match() -> std::io::Result<()> {
    let home = TempDir::new().unwrap();
    let thread_name = "shared-name";

    let cwd_a = home.path().join("workspace-a");
    let cwd_b = home.path().join("workspace-b");
    std::fs::create_dir_all(&cwd_a)?;
    std::fs::create_dir_all(&cwd_b)?;

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    let dir_a = project_sessions_root(home.path(), cwd_a.as_path()).join("2024/01/01");
    let dir_b = project_sessions_root(home.path(), cwd_b.as_path()).join("2024/01/01");
    std::fs::create_dir_all(&dir_a)?;
    std::fs::create_dir_all(&dir_b)?;

    let rollout_a = write_minimal_rollout_with_id_at(dir_a.as_path(), id_a, cwd_a.as_path());
    let _rollout_b = write_minimal_rollout_with_id_at(dir_b.as_path(), id_b, cwd_b.as_path());

    // No project-local index for cwd_a, so lookup must fall back to global history.
    // Newest global match points to cwd_b; older global match points to cwd_a.
    let global_index = home.path().join("session_index.jsonl");
    std::fs::write(
        &global_index,
        format!(
            "{}\n{}\n",
            serde_json::json!({
                "id": id_a,
                "thread_name": thread_name,
                "updated_at": "2024-01-01T00:00:00Z"
            }),
            serde_json::json!({
                "id": id_b,
                "thread_name": thread_name,
                "updated_at": "2024-01-02T00:00:00Z"
            })
        ),
    )?;

    let found = find_thread_path_by_name_str_in_cwd(home.path(), cwd_a.as_path(), thread_name)
        .await?
        .expect("expected lookup to find an older global match for cwd");
    assert_eq!(found, rollout_a);
    Ok(())
}

#[tokio::test]
async fn find_archived_locates_rollout_file_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id_in_subdir(home.path(), "archived_sessions", id);

    let found = find_archived_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}
