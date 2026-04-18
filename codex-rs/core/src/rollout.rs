use crate::config::Config;
use codex_protocol::ThreadId;
use std::path::Path;
use std::path::PathBuf;

pub use codex_rollout::ARCHIVED_SESSIONS_SUBDIR;
pub use codex_rollout::INTERACTIVE_SESSION_SOURCES;
pub use codex_rollout::RolloutRecorder;
pub use codex_rollout::RolloutRecorderParams;
pub use codex_rollout::SESSIONS_SUBDIR;
pub use codex_rollout::SessionMeta;
pub use codex_rollout::append_thread_name;
pub use codex_rollout::find_archived_thread_path_by_id_str;
#[deprecated(note = "use find_thread_path_by_id_str")]
pub use codex_rollout::find_conversation_path_by_id_str;
pub use codex_rollout::find_thread_name_by_id;
pub use codex_rollout::find_thread_path_by_id_str;
pub use codex_rollout::find_thread_path_by_name_str;
pub use codex_rollout::rollout_date_parts;

pub fn session_storage_roots(primary_root: &Path) -> Vec<PathBuf> {
    vec![primary_root.to_path_buf()]
}

pub fn session_lookup_roots(primary_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![primary_root.to_path_buf()];
    if let Ok(upstream_root) = codex_utils_home_dir::find_upstream_codex_home()
        && upstream_root != primary_root
        && upstream_root.exists()
    {
        roots.push(upstream_root);
    }
    roots
}

pub fn storage_root_for_rollout_path(primary_root: &Path, rollout_path: &Path) -> Option<PathBuf> {
    session_lookup_roots(primary_root).into_iter().find(|root| {
        rollout_path.starts_with(root.join(SESSIONS_SUBDIR))
            || rollout_path.starts_with(root.join(ARCHIVED_SESSIONS_SUBDIR))
    })
}

pub async fn find_thread_path_by_id_str_across_roots(
    primary_root: &Path,
    thread_id: &str,
) -> std::io::Result<Option<PathBuf>> {
    for root in session_lookup_roots(primary_root) {
        if let Some(path) = codex_rollout::find_thread_path_by_id_str(&root, thread_id).await? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub async fn find_archived_thread_path_by_id_str_across_roots(
    primary_root: &Path,
    thread_id: &str,
) -> std::io::Result<Option<PathBuf>> {
    for root in session_lookup_roots(primary_root) {
        if let Some(path) =
            codex_rollout::find_archived_thread_path_by_id_str(&root, thread_id).await?
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub async fn find_thread_name_by_id_across_roots(
    primary_root: &Path,
    thread_id: &ThreadId,
) -> std::io::Result<Option<String>> {
    for root in session_lookup_roots(primary_root) {
        if let Some(name) = codex_rollout::find_thread_name_by_id(&root, thread_id).await? {
            return Ok(Some(name));
        }
    }
    Ok(None)
}

pub async fn find_thread_path_by_name_str_across_roots(
    primary_root: &Path,
    thread_name: &str,
) -> std::io::Result<Option<PathBuf>> {
    for root in session_lookup_roots(primary_root) {
        if let Some(path) = codex_rollout::find_thread_path_by_name_str(&root, thread_name).await? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

impl codex_rollout::RolloutConfigView for Config {
    fn codex_home(&self) -> &std::path::Path {
        self.codex_home.as_path()
    }

    fn sqlite_home(&self) -> &std::path::Path {
        self.sqlite_home.as_path()
    }

    fn cwd(&self) -> &std::path::Path {
        self.cwd.as_path()
    }

    fn model_provider_id(&self) -> &str {
        self.model_provider_id.as_str()
    }

    fn generate_memories(&self) -> bool {
        self.memories.generate_memories
    }
}

pub mod list {
    pub use codex_rollout::list::*;
}

pub(crate) mod metadata {
    pub(crate) use codex_rollout::metadata::builder_from_items;
}

pub mod policy {
    pub use codex_rollout::policy::*;
}

pub mod recorder {
    pub use codex_rollout::recorder::*;
}

pub mod session_index {
    pub use codex_rollout::session_index::*;
}

pub(crate) use crate::session_rollout_init_error::map_session_init_error;

pub(crate) mod truncation {
    pub(crate) use crate::thread_rollout_truncation::*;
}
