use dirs::home_dir;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const CODEXPILOT_HOME_ENV_VAR: &str = "CODEXPILOT_HOME";
const CODEX_SQLITE_HOME_ENV_VAR: &str = "CODEX_SQLITE_HOME";
const CODEXPILOT_SQLITE_HOME_ENV_VAR: &str = "CODEXPILOT_SQLITE_HOME";
const CODEX_HOME_DIR_NAME: &str = ".codex";
const CODEXPILOT_HOME_DIR_NAME: &str = ".codexpilot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppHome {
    Codex,
    CodexPilot,
}

impl AppHome {
    fn home_env_var(self) -> &'static str {
        match self {
            Self::Codex => CODEX_HOME_ENV_VAR,
            Self::CodexPilot => CODEXPILOT_HOME_ENV_VAR,
        }
    }

    fn sqlite_home_env_var(self) -> &'static str {
        match self {
            Self::Codex => CODEX_SQLITE_HOME_ENV_VAR,
            Self::CodexPilot => CODEXPILOT_SQLITE_HOME_ENV_VAR,
        }
    }

    fn default_dir_name(self) -> &'static str {
        match self {
            Self::Codex => CODEX_HOME_DIR_NAME,
            Self::CodexPilot => CODEXPILOT_HOME_DIR_NAME,
        }
    }
}

pub fn current_app_home() -> AppHome {
    app_home_for_exe_name(current_exe_name().as_deref())
}

fn app_home_for_exe_name(exe_name: Option<&str>) -> AppHome {
    if exe_name.is_some_and(|name| name.starts_with("codexpilot")) {
        AppHome::CodexPilot
    } else {
        AppHome::Codex
    }
}

pub fn current_app_is_codexpilot() -> bool {
    current_app_home() == AppHome::CodexPilot
}

pub fn current_app_command_name() -> &'static str {
    match current_app_home() {
        AppHome::Codex => "codex",
        AppHome::CodexPilot => "codexpilot",
    }
}

pub fn current_app_display_name() -> &'static str {
    match current_app_home() {
        AppHome::Codex => "OpenAI Codex",
        AppHome::CodexPilot => "CodexPilot",
    }
}

pub fn current_app_home_env_var() -> &'static str {
    current_app_home().home_env_var()
}

pub fn current_app_sqlite_home_env_var() -> &'static str {
    current_app_home().sqlite_home_env_var()
}

pub fn find_upstream_codex_home() -> std::io::Result<PathBuf> {
    let codex_home_env = std::env::var(CODEX_HOME_ENV_VAR)
        .ok()
        .filter(|val| !val.is_empty());
    if let Some(codex_home_env) = codex_home_env.as_deref()
        && let Ok(path) = find_home_from_env(Some(codex_home_env), AppHome::Codex)
    {
        return Ok(path);
    }
    find_home_from_env(/*codex_home_env*/ None, AppHome::Codex)
}

/// Returns the path to the current app's configuration directory.
///
/// Upstream `codex` uses `CODEX_HOME`/`~/.codex`, while `codexpilot` uses
/// `CODEXPILOT_HOME`/`~/.codexpilot`.
pub fn find_codex_home() -> std::io::Result<PathBuf> {
    let app_home = current_app_home();
    let codex_home_env = std::env::var(app_home.home_env_var())
        .ok()
        .filter(|val| !val.is_empty());
    find_home_from_env(codex_home_env.as_deref(), app_home)
}

fn current_exe_name() -> Option<String> {
    std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .map(ToOwned::to_owned)
}

fn find_home_from_env(codex_home_env: Option<&str>, app_home: AppHome) -> std::io::Result<PathBuf> {
    let env_var = app_home.home_env_var();
    match codex_home_env {
        Some(val) => {
            let path = PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{env_var} points to {val:?}, but that path does not exist"),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read {env_var} {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{env_var} points to {val:?}, but that path is not a directory"),
                ))
            } else {
                path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize {env_var} {val:?}: {err}"),
                    )
                })
            }
        }
        None => {
            let mut p = home_dir().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find home directory",
                )
            })?;
            p.push(app_home.default_dir_name());
            Ok(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppHome;
    use super::app_home_for_exe_name;
    use super::find_home_from_env;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn codexpilot_executable_name_selects_codexpilot_home() {
        assert_eq!(
            app_home_for_exe_name(Some("codexpilot")),
            AppHome::CodexPilot
        );
        assert_eq!(
            app_home_for_exe_name(Some("codexpilot-dev")),
            AppHome::CodexPilot
        );
        assert_eq!(app_home_for_exe_name(Some("codex")), AppHome::Codex);
        assert_eq!(
            app_home_for_exe_name(Some("codex-app-server")),
            AppHome::Codex
        );
        assert_eq!(app_home_for_exe_name(None), AppHome::Codex);
    }

    #[test]
    fn find_codex_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-codex-home");
        let missing_str = missing
            .to_str()
            .expect("missing codex home path should be valid utf-8");

        let err =
            find_home_from_env(Some(missing_str), AppHome::Codex).expect_err("missing CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("CODEX_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codexpilot_home_env_missing_path_mentions_codexpilot_env_var() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-codexpilot-home");
        let missing_str = missing
            .to_str()
            .expect("missing codexpilot home path should be valid utf-8");

        let err = find_home_from_env(Some(missing_str), AppHome::CodexPilot)
            .expect_err("missing CODEXPILOT_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("CODEXPILOT_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("codex-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file codex home path should be valid utf-8");

        let err = find_home_from_env(Some(file_str), AppHome::Codex).expect_err("file CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp codex home path should be valid utf-8");

        let resolved =
            find_home_from_env(Some(temp_str), AppHome::Codex).expect("valid CODEX_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_codex_home_without_env_uses_default_home_dir() {
        let resolved = find_home_from_env(/*codex_home_env*/ None, AppHome::Codex)
            .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(".codex");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_codexpilot_home_without_env_uses_default_home_dir() {
        let resolved = find_home_from_env(/*codex_home_env*/ None, AppHome::CodexPilot)
            .expect("default CODEXPILOT_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(".codexpilot");
        assert_eq!(resolved, expected);
    }
}
