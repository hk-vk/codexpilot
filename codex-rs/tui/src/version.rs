use serde::Deserialize;
use std::sync::OnceLock;

/// The current Codex CLI version as embedded at compile time.
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
const VERSION_FILENAME: &str = "version.json";

static DISPLAY_CLI_VERSION: OnceLock<String> = OnceLock::new();

#[derive(Deserialize)]
struct VersionInfo {
    latest_version: String,
}

pub fn display_cli_version() -> &'static str {
    DISPLAY_CLI_VERSION
        .get_or_init(resolve_display_cli_version)
        .as_str()
}

fn resolve_display_cli_version() -> String {
    if CODEX_CLI_VERSION != "0.0.0" {
        return CODEX_CLI_VERSION.to_string();
    }

    let current_home = codex_core::config::find_codex_home().ok();
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

    CODEX_CLI_VERSION.to_string()
}
