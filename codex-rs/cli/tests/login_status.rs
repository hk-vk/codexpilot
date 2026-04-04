use std::path::Path;

use anyhow::Result;
use codex_login::AuthCredentialsStoreMode;
use codex_login::login_with_api_key;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn codexpilot_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codexpilot")?);
    cmd.env("HOME", codex_home)
        .env("CODEXPILOT_HOME", codex_home)
        .env_remove("CODEX_HOME")
        .env_remove("CODEXPILOT_SQLITE_HOME");
    Ok(cmd)
}

fn write_file_auth_config(codex_home: &Path) -> Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )?;
    Ok(())
}

fn write_github_copilot_auth(codex_home: &Path, enterprise_domain: Option<&str>) -> Result<()> {
    let enterprise_domain = enterprise_domain
        .map(|domain| format!("\"{domain}\""))
        .unwrap_or_else(|| "null".to_string());
    std::fs::write(
        codex_home.join("github-copilot-auth.json"),
        format!(
            "{{\n  \"github_access_token\": \"gho_test\",\n  \"copilot_access_token\": \"copilot_test\",\n  \"copilot_token_expires_at\": \"2099-01-01T00:00:00Z\",\n  \"api_base_url\": \"https://api.githubcopilot.com\",\n  \"enterprise_domain\": {enterprise_domain},\n  \"saved_at\": \"2099-01-01T00:00:00Z\"\n}}\n"
        ),
    )?;
    Ok(())
}

#[test]
fn login_status_reports_github_copilot_when_only_copilot_auth_exists() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_file_auth_config(codex_home.path())?;

    write_github_copilot_auth(codex_home.path(), Some("ghe.example.com"))?;

    let mut cmd = codexpilot_command(codex_home.path())?;
    cmd.args(["login", "status"])
        .assert()
        .success()
        .stderr(contains("Logged in using GitHub Copilot (ghe.example.com)"));

    Ok(())
}

#[test]
fn login_status_prefers_codex_auth_when_both_codex_and_copilot_exist() -> Result<()> {
    let codex_home = TempDir::new()?;
    write_file_auth_config(codex_home.path())?;

    login_with_api_key(
        codex_home.path(),
        "sk-test-1234567890",
        AuthCredentialsStoreMode::File,
    )?;
    write_github_copilot_auth(codex_home.path(), None)?;

    let mut cmd = codexpilot_command(codex_home.path())?;
    cmd.args(["login", "status"])
        .assert()
        .success()
        .stderr(contains("Logged in using an API key").and(contains("GitHub Copilot").not()));

    Ok(())
}
