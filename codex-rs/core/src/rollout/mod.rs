//! Rollout module: persistence and discovery of session rollout files.

use codex_protocol::protocol::SessionSource;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

pub const SESSIONS_SUBDIR: &str = "sessions";
pub const ARCHIVED_SESSIONS_SUBDIR: &str = "archived_sessions";
pub const PROJECTS_SUBDIR: &str = "projects";
const PROJECT_SLUG_MAX_COMPONENT_BYTES: usize = 255;
const PROJECT_SLUG_HASH_LEN: usize = 7;
const PROJECT_SLUG_SEPARATOR: &str = "--";
const PROJECT_SLUG_FALLBACK_PREFIX: &str = "project";
pub const INTERACTIVE_SESSION_SOURCES: &[SessionSource] =
    &[SessionSource::Cli, SessionSource::VSCode];

/// Build a stable project key from a cwd path for filesystem hierarchy.
///
/// This mirrors the practical shape used by other coding tools:
/// `/root/git/codex` -> `-root-git-codex`.
pub fn project_slug_for_cwd(cwd: &Path) -> String {
    // Normalize path first so semantically equivalent paths map to the same key.
    let normalized_cwd =
        crate::path_utils::normalize_for_path_comparison(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let normalized_cwd = normalized_cwd.to_string_lossy().into_owned();

    // Keep a readable path-derived prefix.
    let mut slug = String::new();
    for ch in normalized_cwd.chars() {
        match ch {
            '/' | '\\' | ':' => slug.push('-'),
            c if c.is_ascii_alphanumeric() => slug.push(c.to_ascii_lowercase()),
            '-' | '_' | '.' => slug.push(ch),
            _ => slug.push('-'),
        }
    }
    // Add a short stable hash suffix to avoid collisions in the readable prefix.
    // 7 hex chars gives 28 bits of space while keeping paths concise.
    let hash_uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, normalized_cwd.as_bytes());
    let hash_hex = hash_uuid.simple().to_string();
    let short_hash = &hash_hex[..PROJECT_SLUG_HASH_LEN];

    if slug.is_empty() {
        slug.push_str(PROJECT_SLUG_FALLBACK_PREFIX);
    }

    let suffix_len = PROJECT_SLUG_SEPARATOR.len() + PROJECT_SLUG_HASH_LEN;
    let max_prefix_bytes = PROJECT_SLUG_MAX_COMPONENT_BYTES.saturating_sub(suffix_len);
    if slug.len() > max_prefix_bytes {
        slug.truncate(max_prefix_bytes);
        slug = slug.trim_end_matches('-').to_string();
        if slug.is_empty() {
            slug.push_str(PROJECT_SLUG_FALLBACK_PREFIX);
        }
    }

    format!("{slug}{PROJECT_SLUG_SEPARATOR}{short_hash}")
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
