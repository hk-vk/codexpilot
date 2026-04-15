use serde::Deserialize;
use std::sync::OnceLock;

/// The current Codex CLI version as embedded at compile time.
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
const VERSION_FILENAME: &str = "version.json";
const INSTALLED_PACKAGE_VERSION_ENV_VAR: &str = "CODEX_INSTALLED_PACKAGE_VERSION";

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

    let installed_version = std::env::var(INSTALLED_PACKAGE_VERSION_ENV_VAR).ok();
    resolve_display_cli_version_from_sources(
        CODEX_CLI_VERSION,
        installed_version.as_deref(),
        &candidates,
    )
}

fn resolve_display_cli_version_from_sources(
    embedded_version: &str,
    installed_version: Option<&str>,
    candidates: &[std::path::PathBuf],
) -> String {
    if let Some(installed_version) = installed_version.map(str::trim)
        && !installed_version.is_empty()
    {
        return installed_version.to_string();
    }

    if embedded_version != "0.0.0" {
        return embedded_version.to_string();
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

    embedded_version.to_string()
}

#[cfg(test)]
mod tests {
    use super::resolve_display_cli_version_from_sources;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn embedded_non_placeholder_version_wins() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(
            dir.path().join("version.json"),
            r#"{"latest_version":"0.118.0"}"#,
        )
        .expect("write version file");

        assert_eq!(
            resolve_display_cli_version_from_sources("0.200.0", None, &[dir.path().to_path_buf()]),
            "0.200.0"
        );
    }

    #[test]
    fn placeholder_version_uses_first_non_empty_version_file() {
        let current_home = TempDir::new().expect("tempdir");
        let upstream_home = TempDir::new().expect("tempdir");
        std::fs::write(
            current_home.path().join("version.json"),
            r#"{"latest_version":""}"#,
        )
        .expect("write empty version file");
        std::fs::write(
            upstream_home.path().join("version.json"),
            r#"{"latest_version":"0.118.0"}"#,
        )
        .expect("write upstream version file");

        assert_eq!(
            resolve_display_cli_version_from_sources(
                "0.0.0",
                None,
                &[
                    current_home.path().to_path_buf(),
                    upstream_home.path().to_path_buf(),
                ],
            ),
            "0.118.0"
        );
    }

    #[test]
    fn placeholder_version_falls_back_when_no_candidate_file_matches() {
        let missing = PathBuf::from("/tmp/definitely-missing-codexpilot-version-home");
        assert_eq!(
            resolve_display_cli_version_from_sources("0.0.0", None, &[missing]),
            "0.0.0"
        );
    }

    #[test]
    fn installed_package_version_overrides_placeholder_fallback() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(
            dir.path().join("version.json"),
            r#"{"latest_version":"0.118.0"}"#,
        )
        .expect("write version file");

        assert_eq!(
            resolve_display_cli_version_from_sources(
                "0.0.0",
                Some("0.0.0-alpha.1"),
                &[dir.path().to_path_buf()],
            ),
            "0.0.0-alpha.1"
        );
    }
}
