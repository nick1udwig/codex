//! Rollout module: persistence and discovery of session rollout files.

use codex_protocol::protocol::SessionSource;
use std::path::Path;
use std::path::PathBuf;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";
pub const PROJECTS_SUBDIR: &str = "projects";
pub const INTERACTIVE_SESSION_SOURCES: &[SessionSource] =
    &[SessionSource::Cli, SessionSource::VSCode];

/// Build a stable project key from a cwd path for filesystem hierarchy.
///
/// This mirrors the practical shape used by other coding tools:
/// `/root/git/codex` -> `-root-git-codex`.
pub fn project_slug_for_cwd(cwd: &Path) -> String {
    let mut slug = String::new();
    for ch in cwd.to_string_lossy().chars() {
        match ch {
            '/' | '\\' | ':' => slug.push('-'),
            '\0' => slug.push('_'),
            _ => slug.push(ch),
        }
    }
    if slug.is_empty() {
        "-".to_string()
    } else {
        slug
    }
}

pub fn project_sessions_root(codex_home: &Path, cwd: &Path) -> PathBuf {
    codex_home
        .join(SESSIONS_SUBDIR)
        .join(PROJECTS_SUBDIR)
        .join(project_slug_for_cwd(cwd))
}

pub(crate) mod error;
pub mod list;
pub(crate) mod metadata;
pub(crate) mod policy;
pub mod recorder;
pub(crate) mod session_index;
pub(crate) mod truncation;

pub use codex_protocol::protocol::SessionMeta;
pub(crate) use error::map_session_init_error;
pub use list::find_archived_thread_path_by_id_str;
pub use list::find_thread_path_by_id_str;
#[deprecated(note = "use find_thread_path_by_id_str")]
pub use list::find_thread_path_by_id_str as find_conversation_path_by_id_str;
pub use list::rollout_date_parts;
pub use recorder::RolloutRecorder;
pub use recorder::RolloutRecorderParams;
pub use session_index::find_thread_name_by_id;
pub use session_index::find_thread_path_by_name_str;
pub use session_index::find_thread_path_by_name_str_in_cwd;

#[cfg(test)]
pub mod tests;
