//! CLI login commands and their direct-user observability surfaces.
//!
//! The TUI path already installs a broader tracing stack with feedback, OpenTelemetry, and other
//! interactive-session layers. Direct `codex login` intentionally does less: it preserves the
//! existing stderr/browser UX and adds only a small file-backed tracing layer for login-specific
//! targets. Keeping that setup local avoids pulling the TUI's session-oriented logging machinery
//! into a one-shot CLI command while still producing a durable `codex-login.log` artifact that
//! support can request from users.

use codex_app_server_protocol::AuthMode;
use codex_core::config::Config;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_login::AuthCredentialsStoreMode;
use codex_login::CLIENT_ID;
use codex_login::CodexAuth;
use codex_login::ServerOptions;
use codex_login::github_copilot::build_github_copilot_client;
use codex_login::github_copilot::exchange_github_copilot_access_token;
use codex_login::github_copilot::github_copilot_access_token_is_stale;
use codex_login::github_copilot::normalize_github_domain;
use codex_login::github_copilot::poll_github_device_access_token;
use codex_login::github_copilot::refresh_github_copilot_auth;
use codex_login::github_copilot::request_github_device_code;
use codex_login::github_copilot_storage::GitHubCopilotAuth;
use codex_login::github_copilot_storage::delete_github_copilot_auth;
use codex_login::github_copilot_storage::load_github_copilot_auth;
use codex_login::github_copilot_storage::save_github_copilot_auth;
use codex_login::login_with_api_key;
use codex_login::logout;
use codex_login::run_device_code_login;
use codex_login::run_login_server;
use codex_protocol::config_types::ForcedLoginMethod;
use codex_utils_cli::CliConfigOverrides;
use std::fs::OpenOptions;
use std::io::IsTerminal;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const CHATGPT_LOGIN_DISABLED_MESSAGE: &str =
    "ChatGPT login is disabled. Use API key login instead.";
const API_KEY_LOGIN_DISABLED_MESSAGE: &str =
    "API key login is disabled. Use ChatGPT login instead.";
const LOGIN_SUCCESS_MESSAGE: &str = "Successfully logged in";

/// Installs a small file-backed tracing layer for direct `codex login` flows.
///
/// This deliberately duplicates a narrow slice of the TUI logging setup instead of reusing it
/// wholesale. The TUI stack includes session-oriented layers that are valuable for interactive
/// runs but unnecessary for a one-shot login command. Keeping the direct CLI path local lets this
/// command produce a durable `codex-login.log` artifact without coupling it to the TUI's broader
/// telemetry and feedback initialization.
fn init_login_file_logging(config: &Config) -> Option<WorkerGuard> {
    let log_dir = match codex_core::config::log_dir(config) {
        Ok(log_dir) => log_dir,
        Err(err) => {
            eprintln!("Warning: failed to resolve login log directory: {err}");
            return None;
        }
    };

    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        eprintln!(
            "Warning: failed to create login log directory {}: {err}",
            log_dir.display()
        );
        return None;
    }

    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_path = log_dir.join("codex-login.log");
    let log_file = match log_file_opts.open(&log_path) {
        Ok(log_file) => log_file,
        Err(err) => {
            eprintln!(
                "Warning: failed to open login log file {}: {err}",
                log_path.display()
            );
            return None;
        }
    };

    let (non_blocking, guard) = non_blocking(log_file);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("codex_cli=info,codex_core=info,codex_login=info"));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_target(true)
        .with_ansi(false)
        .with_filter(env_filter);

    // Direct `codex login` otherwise relies on ephemeral stderr and browser output.
    // Persist the same login targets to a file so support can inspect auth failures
    // without reproducing them through TUI or app-server.
    if let Err(err) = tracing_subscriber::registry().with(file_layer).try_init() {
        eprintln!(
            "Warning: failed to initialize login log file {}: {err}",
            log_path.display()
        );
        return None;
    }

    Some(guard)
}

fn print_login_server_start(actual_port: u16, auth_url: &str) {
    eprintln!(
        "Starting local login server on http://localhost:{actual_port}.\nIf your browser did not open, navigate to this URL to authenticate:\n\n{auth_url}\n\nOn a remote or headless machine? Use `codex login --device-auth` instead."
    );
}

pub async fn login_with_chatgpt(
    codex_home: PathBuf,
    forced_chatgpt_workspace_id: Option<String>,
    cli_auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    let opts = ServerOptions::new(
        codex_home,
        CLIENT_ID.to_string(),
        forced_chatgpt_workspace_id,
        cli_auth_credentials_store_mode,
    );
    let server = run_login_server(opts)?;

    print_login_server_start(server.actual_port, &server.auth_url);

    server.block_until_done().await
}

pub async fn run_login_with_chatgpt(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting browser login flow");

    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();

    match login_with_chatgpt(
        config.codex_home,
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    )
    .await
    {
        Ok(_) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn run_login_with_api_key(
    cli_config_overrides: CliConfigOverrides,
    api_key: String,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting api key login flow");

    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Chatgpt)) {
        eprintln!("{API_KEY_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    match login_with_api_key(
        &config.codex_home,
        &api_key,
        config.cli_auth_credentials_store_mode,
    ) {
        Ok(_) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in: {e}");
            std::process::exit(1);
        }
    }
}

pub fn read_api_key_from_stdin() -> String {
    let mut stdin = std::io::stdin();

    if stdin.is_terminal() {
        eprintln!(
            "--with-api-key expects the API key on stdin. Try piping it, e.g. `printenv OPENAI_API_KEY | codex login --with-api-key`."
        );
        std::process::exit(1);
    }

    eprintln!("Reading API key from stdin...");

    let mut buffer = String::new();
    if let Err(err) = stdin.read_to_string(&mut buffer) {
        eprintln!("Failed to read API key from stdin: {err}");
        std::process::exit(1);
    }

    let api_key = buffer.trim().to_string();
    if api_key.is_empty() {
        eprintln!("No API key provided via stdin.");
        std::process::exit(1);
    }

    api_key
}

/// Login using the OAuth device code flow.
pub async fn run_login_with_device_code(
    cli_config_overrides: CliConfigOverrides,
    issuer_base_url: Option<String>,
    client_id: Option<String>,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting device code login flow");
    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }
    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();
    let mut opts = ServerOptions::new(
        config.codex_home,
        client_id.unwrap_or(CLIENT_ID.to_string()),
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    );
    if let Some(iss) = issuer_base_url {
        opts.issuer = iss;
    }
    match run_device_code_login(opts).await {
        Ok(()) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in with device code: {e}");
            std::process::exit(1);
        }
    }
}

/// Prefers device-code login (with `open_browser = false`) when headless environment is detected, but keeps
/// `codex login` working in environments where device-code may be disabled/feature-gated.
/// If `run_device_code_login` returns `ErrorKind::NotFound` ("device-code unsupported"), this
/// falls back to starting the local browser login server.
pub async fn run_login_with_device_code_fallback_to_browser(
    cli_config_overrides: CliConfigOverrides,
    issuer_base_url: Option<String>,
    client_id: Option<String>,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting login flow with device code fallback");
    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();
    let mut opts = ServerOptions::new(
        config.codex_home,
        client_id.unwrap_or(CLIENT_ID.to_string()),
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    );
    if let Some(iss) = issuer_base_url {
        opts.issuer = iss;
    }
    opts.open_browser = false;

    match run_device_code_login(opts.clone()).await {
        Ok(()) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Device code login is not enabled; falling back to browser login.");
                match run_login_server(opts) {
                    Ok(server) => {
                        print_login_server_start(server.actual_port, &server.auth_url);
                        match server.block_until_done().await {
                            Ok(()) => {
                                eprintln!("{LOGIN_SUCCESS_MESSAGE}");
                                std::process::exit(0);
                            }
                            Err(e) => {
                                eprintln!("Error logging in: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error logging in: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Error logging in with device code: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn current_codex_command_path() -> String {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "codex".to_string())
}

fn github_copilot_provider_config_block(base_url: &str, command_path: &str) -> String {
    format!(
        "[model_providers.github-copilot]\nname = \"GitHub Copilot\"\nbase_url = \"{base_url}\"\nwire_api = \"responses\"\n\n[model_providers.github-copilot.auth]\ncommand = \"{command_path}\"\nargs = [\"login\", \"github-copilot-token\"]\nrefresh_interval_ms = 240000\n"
    )
}

fn ensure_github_copilot_provider_config(
    codex_home: &Path,
    base_url: &str,
) -> std::io::Result<bool> {
    let config_file = codex_home.join("config.toml");
    let existing = match std::fs::read_to_string(&config_file) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };

    if existing.contains("[model_providers.github-copilot]") {
        return Ok(false);
    }

    if let Some(parent) = config_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config_file)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    if !existing.is_empty() {
        file.write_all(b"\n")?;
    }
    let command_path = current_codex_command_path();
    file.write_all(github_copilot_provider_config_block(base_url, &command_path).as_bytes())?;
    file.flush()?;
    Ok(true)
}

async fn ensure_github_copilot_provider_ready(
    codex_home: &Path,
    base_url: &str,
) -> std::io::Result<()> {
    let _ = ensure_github_copilot_provider_config(codex_home, base_url)?;
    ConfigEditsBuilder::new(codex_home)
        .set_model_provider(Some("github-copilot"))
        .apply()
        .await
        .map_err(std::io::Error::other)
}

pub async fn run_login_with_github_copilot(
    cli_config_overrides: CliConfigOverrides,
    enterprise_url: Option<String>,
    write_config: bool,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!(enterprise_url = ?enterprise_url.as_deref(), "starting GitHub Copilot login flow");

    let enterprise_domain = match enterprise_url {
        Some(value) => match normalize_github_domain(&value) {
            Some(domain) => Some(domain),
            None => {
                eprintln!("Invalid GitHub Enterprise URL or domain: {value}");
                std::process::exit(1);
            }
        },
        None => None,
    };

    let client = match build_github_copilot_client() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Error building GitHub Copilot login client: {err}");
            std::process::exit(1);
        }
    };

    let device_code =
        match request_github_device_code(&client, enterprise_domain.as_deref(), None).await {
            Ok(device_code) => device_code,
            Err(err) => {
                eprintln!("Error starting GitHub Copilot device login: {err}");
                std::process::exit(1);
            }
        };

    eprintln!(
        "Open this URL and enter the code:\n\n{}\n\nCode: {}\n",
        device_code.verification_uri, device_code.user_code
    );
    eprintln!("Waiting for GitHub authorization...");

    let github_access_token = match poll_github_device_access_token(
        &client,
        &device_code,
        enterprise_domain.as_deref(),
        None,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            eprintln!("Error completing GitHub device login: {err}");
            std::process::exit(1);
        }
    };

    let copilot_access_token = match exchange_github_copilot_access_token(
        &client,
        &github_access_token,
        enterprise_domain.as_deref(),
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            eprintln!("Error exchanging GitHub Copilot token: {err}");
            std::process::exit(1);
        }
    };

    let auth = GitHubCopilotAuth::new(
        github_access_token,
        copilot_access_token.token,
        copilot_access_token.expires_at,
        copilot_access_token.api_base_url,
        enterprise_domain,
    );

    match save_github_copilot_auth(&config.codex_home, &auth) {
        Ok(()) => {
            if let Err(err) =
                ensure_github_copilot_provider_ready(&config.codex_home, &auth.api_base_url).await
            {
                eprintln!("Error preparing GitHub Copilot provider config: {err}");
                std::process::exit(1);
            }
            if write_config {
                eprintln!("Ensured GitHub Copilot provider config in ~/.codex/config.toml");
            }
            eprintln!("Successfully logged in with GitHub Copilot");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("Error saving GitHub Copilot credentials: {err}");
            std::process::exit(1);
        }
    }
}

pub async fn run_print_github_copilot_provider_config(
    cli_config_overrides: CliConfigOverrides,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let base_url = match load_github_copilot_auth(&config.codex_home) {
        Ok(Some(auth)) => auth.api_base_url,
        Ok(None) => "https://api.githubcopilot.com".to_string(),
        Err(err) => {
            eprintln!("Error loading GitHub Copilot credentials: {err}");
            std::process::exit(1);
        }
    };

    let command_path = current_codex_command_path();

    println!("[model_providers.github-copilot]");
    println!("name = \"GitHub Copilot\"");
    println!("base_url = \"{base_url}\"");
    println!("wire_api = \"responses\"");
    println!("http_headers = {{ Openai-Intent = \"conversation-edits\" }}");
    println!();
    println!("[model_providers.github-copilot.auth]");
    println!("command = \"{command_path}\"");
    println!("args = [\"login\", \"github-copilot-token\"]");
    println!("refresh_interval_ms = 240000");
    println!();
    println!("model_provider = \"github-copilot\"");
    std::process::exit(0);
}

pub async fn run_print_github_copilot_access_token(
    cli_config_overrides: CliConfigOverrides,
    refresh: bool,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let auth = match load_github_copilot_auth(&config.codex_home) {
        Ok(Some(auth)) => auth,
        Ok(None) => {
            eprintln!("GitHub Copilot is not logged in");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("Error loading GitHub Copilot credentials: {err}");
            std::process::exit(1);
        }
    };

    let auth = if refresh || github_copilot_access_token_is_stale(&auth) {
        let client = match build_github_copilot_client() {
            Ok(client) => client,
            Err(err) => {
                eprintln!("Error building GitHub Copilot client: {err}");
                std::process::exit(1);
            }
        };

        match refresh_github_copilot_auth(&client, &auth).await {
            Ok(refreshed) => {
                if let Err(err) = save_github_copilot_auth(&config.codex_home, &refreshed) {
                    eprintln!("Error saving refreshed GitHub Copilot credentials: {err}");
                    std::process::exit(1);
                }
                refreshed
            }
            Err(err) => {
                eprintln!("Error refreshing GitHub Copilot access token: {err}");
                std::process::exit(1);
            }
        }
    } else {
        auth
    };

    println!("{}", auth.copilot_access_token);
    std::process::exit(0);
}

pub async fn run_login_status(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    match CodexAuth::from_auth_storage(&config.codex_home, config.cli_auth_credentials_store_mode) {
        Ok(Some(auth)) => match auth.auth_mode() {
            AuthMode::ApiKey => match auth.get_token() {
                Ok(api_key) => {
                    eprintln!("Logged in using an API key - {}", safe_format_key(&api_key));
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Unexpected error retrieving API key: {e}");
                    std::process::exit(1);
                }
            },
            AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens => {
                eprintln!("Logged in using ChatGPT");
                std::process::exit(0);
            }
        },
        Ok(None) => match load_github_copilot_auth(&config.codex_home) {
            Ok(Some(auth)) => {
                let host = auth.enterprise_domain.as_deref().unwrap_or("github.com");
                eprintln!("Logged in using GitHub Copilot ({host})");
                std::process::exit(0);
            }
            Ok(None) => {
                eprintln!("Not logged in");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error checking GitHub Copilot login status: {e}");
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error checking login status: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn run_logout(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    let removed_codex_auth =
        match logout(&config.codex_home, config.cli_auth_credentials_store_mode) {
            Ok(removed) => removed,
            Err(e) => {
                eprintln!("Error logging out: {e}");
                std::process::exit(1);
            }
        };

    let removed_copilot_auth = match delete_github_copilot_auth(&config.codex_home) {
        Ok(removed) => removed,
        Err(e) => {
            eprintln!("Error removing GitHub Copilot credentials: {e}");
            std::process::exit(1);
        }
    };

    if removed_codex_auth || removed_copilot_auth {
        eprintln!("Successfully logged out");
        std::process::exit(0);
    }

    eprintln!("Not logged in");
    std::process::exit(0);
}

async fn load_config_or_exit(cli_config_overrides: CliConfigOverrides) -> Config {
    let cli_overrides = match cli_config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    match Config::load_with_cli_overrides(cli_overrides).await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error loading configuration: {e}");
            std::process::exit(1);
        }
    }
}

fn safe_format_key(key: &str) -> String {
    if key.len() <= 13 {
        return "***".to_string();
    }
    let prefix = &key[..8];
    let suffix = &key[key.len() - 5..];
    format!("{prefix}***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::ensure_github_copilot_provider_config;
    use super::github_copilot_provider_config_block;
    use super::safe_format_key;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn formats_long_key() {
        let key = "sk-proj-1234567890ABCDE";
        assert_eq!(safe_format_key(key), "sk-proj-***ABCDE");
    }

    #[test]
    fn short_key_returns_stars() {
        let key = "sk-proj-12345";
        assert_eq!(safe_format_key(key), "***");
    }

    #[test]
    fn github_copilot_provider_config_block_contains_expected_settings() {
        let block =
            github_copilot_provider_config_block("https://api.githubcopilot.com", "/usr/bin/codex");
        assert!(block.contains("[model_providers.github-copilot]"));
        assert!(block.contains("command = \"/usr/bin/codex\""));
        assert!(block.contains("args = [\"login\", \"github-copilot-token\"]"));
    }

    #[test]
    fn ensure_github_copilot_provider_config_appends_only_once() {
        let dir = tempdir().expect("tempdir should exist");

        let wrote =
            ensure_github_copilot_provider_config(dir.path(), "https://api.githubcopilot.com")
                .expect("config write should succeed");
        assert!(wrote);

        let wrote_again =
            ensure_github_copilot_provider_config(dir.path(), "https://api.githubcopilot.com")
                .expect("second config write should succeed");
        assert!(!wrote_again);

        let config = std::fs::read_to_string(dir.path().join("config.toml"))
            .expect("config should be readable");
        assert_eq!(
            config.matches("[model_providers.github-copilot]").count(),
            1
        );
    }
}
