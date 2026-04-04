pub mod cache;
pub mod collaboration_mode_presets;
pub mod config;
pub mod manager;
pub mod model_info;
pub mod model_presets;

use serde::Deserialize;
use std::sync::OnceLock;

pub use codex_app_server_protocol::AuthMode;
pub use codex_login::AuthCredentialsStoreMode;
pub use codex_login::AuthManager;
pub use codex_login::CodexAuth;
pub use codex_model_provider_info::ModelProviderInfo;
pub use codex_model_provider_info::WireApi;
pub use config::ModelsManagerConfig;

static CLIENT_VERSION_WHOLE: OnceLock<String> = OnceLock::new();
const VERSION_FILENAME: &str = "version.json";

#[derive(Deserialize)]
struct VersionInfo {
    latest_version: String,
}

/// Load the bundled model catalog shipped with `codex-models-manager`.
pub fn bundled_models_response()
-> std::result::Result<codex_protocol::openai_models::ModelsResponse, serde_json::Error> {
    serde_json::from_str(include_str!("../models.json"))
}

/// Convert the client version string to a whole version string (e.g. "1.2.3-alpha.4" -> "1.2.3").
pub fn client_version_to_whole() -> String {
    CLIENT_VERSION_WHOLE
        .get_or_init(resolve_client_version_to_whole)
        .clone()
}

fn resolve_client_version_to_whole() -> String {
    let embedded = format!(
        "{}.{}.{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    if embedded != "0.0.0" {
        return embedded;
    }

    let current_home = codex_utils_home_dir::find_codex_home().ok();
    let upstream_home = codex_utils_home_dir::find_upstream_codex_home().ok();
    let mut candidates = Vec::new();
    if let Some(current_home) = current_home {
        candidates.push(current_home);
    }
    if let Some(upstream_home) = upstream_home {
        let already_present = candidates
            .iter()
            .any(|candidate| candidate == &upstream_home);
        if !already_present {
            candidates.push(upstream_home);
        }
    }

    for home in candidates {
        let version_file = home.join(VERSION_FILENAME);
        if let Ok(contents) = std::fs::read_to_string(version_file)
            && let Ok(info) = serde_json::from_str::<VersionInfo>(&contents)
            && !info.latest_version.trim().is_empty()
        {
            return info.latest_version;
        }
    }

    embedded
}
