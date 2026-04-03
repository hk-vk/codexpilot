use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const GITHUB_COPILOT_AUTH_FILE: &str = "github-copilot-auth.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubCopilotAuth {
    pub github_access_token: String,
    pub copilot_access_token: String,
    pub copilot_token_expires_at: DateTime<Utc>,
    pub api_base_url: String,
    pub enterprise_domain: Option<String>,
    pub saved_at: DateTime<Utc>,
}

impl GitHubCopilotAuth {
    pub fn new(
        github_access_token: String,
        copilot_access_token: String,
        copilot_token_expires_at: DateTime<Utc>,
        api_base_url: String,
        enterprise_domain: Option<String>,
    ) -> Self {
        Self {
            github_access_token,
            copilot_access_token,
            copilot_token_expires_at,
            api_base_url,
            enterprise_domain,
            saved_at: Utc::now(),
        }
    }
}

pub fn load_github_copilot_auth(codex_home: &Path) -> io::Result<Option<GitHubCopilotAuth>> {
    let auth_file = github_copilot_auth_file(codex_home);
    let mut file = match File::open(&auth_file) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let auth = serde_json::from_str(&contents).map_err(|err| {
        io::Error::other(format!("failed to parse {}: {err}", auth_file.display()))
    })?;
    Ok(Some(auth))
}

pub fn save_github_copilot_auth(codex_home: &Path, auth: &GitHubCopilotAuth) -> io::Result<()> {
    let auth_file = github_copilot_auth_file(codex_home);
    if let Some(parent) = auth_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(auth).map_err(|err| {
        io::Error::other(format!("failed to serialize GitHub Copilot auth: {err}"))
    })?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(&auth_file)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    Ok(())
}

pub fn delete_github_copilot_auth(codex_home: &Path) -> io::Result<bool> {
    let auth_file = github_copilot_auth_file(codex_home);
    match std::fs::remove_file(&auth_file) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

pub fn github_copilot_auth_file(codex_home: &Path) -> PathBuf {
    codex_home.join(GITHUB_COPILOT_AUTH_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn saves_and_loads_github_copilot_auth() {
        let dir = tempdir().unwrap();
        let auth = GitHubCopilotAuth {
            github_access_token: "gho_test".to_string(),
            copilot_access_token: "copilot_test".to_string(),
            copilot_token_expires_at: DateTime::<Utc>::from_timestamp(1_900_000_000, 0).unwrap(),
            api_base_url: "https://api.githubcopilot.com".to_string(),
            enterprise_domain: Some("ghe.example.com".to_string()),
            saved_at: DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap(),
        };

        save_github_copilot_auth(dir.path(), &auth).unwrap();

        let loaded = load_github_copilot_auth(dir.path()).unwrap();
        assert_eq!(loaded, Some(auth));
    }

    #[test]
    fn delete_github_copilot_auth_reports_absence() {
        let dir = tempdir().unwrap();
        assert_eq!(delete_github_copilot_auth(dir.path()).unwrap(), false);
    }
}
