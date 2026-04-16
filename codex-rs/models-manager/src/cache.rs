use chrono::DateTime;
use chrono::Utc;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use codex_protocol::openai_models::ModelInfo;
use serde::Deserialize;
use serde::Serialize;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tracing::error;
use tracing::info;

/// Manages loading and saving of models cache to disk.
#[derive(Debug)]
pub(crate) struct ModelsCacheManager {
    cache_path: PathBuf,
    cache_ttl: Duration,
    provider_id: String,
}

impl ModelsCacheManager {
    /// Create a new cache manager with the given path and TTL.
    pub(crate) fn new(cache_path: PathBuf, cache_ttl: Duration, provider_id: String) -> Self {
        Self {
            cache_path,
            cache_ttl,
            provider_id,
        }
    }

    /// Attempt to load a fresh cache entry. Returns `None` if the cache doesn't exist or is stale.
    pub(crate) async fn load_fresh(&self, expected_version: &str) -> Option<ModelsCache> {
        info!(
                cache_path = %self.cache_path.display(),
                expected_version,
            "models cache: attempting load_fresh"
        );
        let cache = match self.load().await {
            Ok(cache) => cache?,
            Err(err) => {
                error!("failed to load models cache: {err}");
                return None;
            }
        };
        info!(
            cache_path = %self.cache_path.display(),
            cached_version = ?cache.client_version,
            fetched_at = %cache.fetched_at,
            "models cache: loaded cache file"
        );
        if !cache_version_matches(cache.client_version.as_deref(), expected_version) {
            info!(
                cache_path = %self.cache_path.display(),
                expected_version,
                cached_version = ?cache.client_version,
                "models cache: cache version mismatch"
            );
            return None;
        }
        if !cache_provider_matches(cache.provider_id.as_deref(), self.provider_id.as_str()) {
            info!(
                cache_path = %self.cache_path.display(),
                expected_provider_id = %self.provider_id,
                cached_provider_id = ?cache.provider_id,
                "models cache: cache provider mismatch"
            );
            return None;
        }
        if !cache.is_fresh(self.cache_ttl) {
            info!(
                cache_path = %self.cache_path.display(),
                cache_ttl_secs = self.cache_ttl.as_secs(),
                fetched_at = %cache.fetched_at,
                "models cache: cache is stale"
            );
            return None;
        }
        info!(
            cache_path = %self.cache_path.display(),
            cache_ttl_secs = self.cache_ttl.as_secs(),
            "models cache: cache hit"
        );
        Some(cache)
    }

    /// Persist the cache to disk, creating parent directories as needed.
    pub(crate) async fn persist_cache(
        &self,
        models: &[ModelInfo],
        etag: Option<String>,
        client_version: String,
    ) {
        let cache = ModelsCache {
            fetched_at: Utc::now(),
            etag,
            client_version: Some(client_version),
            provider_id: Some(self.provider_id.clone()),
            models: models.to_vec(),
        };
        if let Err(err) = self.save_internal(&cache).await {
            error!("failed to write models cache: {err}");
        }
    }

    /// Renew the cache TTL by updating the fetched_at timestamp to now.
    pub(crate) async fn renew_cache_ttl(&self) -> io::Result<()> {
        let mut cache = match self.load().await? {
            Some(cache) => cache,
            None => return Err(io::Error::new(ErrorKind::NotFound, "cache not found")),
        };
        cache.fetched_at = Utc::now();
        self.save_internal(&cache).await
    }

    async fn load(&self) -> io::Result<Option<ModelsCache>> {
        load_cache_file(self.cache_path.as_path()).await
    }

    async fn save_internal(&self, cache: &ModelsCache) -> io::Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(cache)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
        fs::write(&self.cache_path, json).await
    }

    #[cfg(test)]
    /// Set the cache TTL.
    pub(crate) fn set_ttl(&mut self, ttl: Duration) {
        self.cache_ttl = ttl;
    }

    #[cfg(test)]
    /// Manipulate cache file for testing. Allows setting a custom fetched_at timestamp.
    pub(crate) async fn manipulate_cache_for_test<F>(&self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut DateTime<Utc>),
    {
        let mut cache = match self.load().await? {
            Some(cache) => cache,
            None => return Err(io::Error::new(ErrorKind::NotFound, "cache not found")),
        };
        f(&mut cache.fetched_at);
        self.save_internal(&cache).await
    }

    #[cfg(test)]
    /// Mutate the full cache contents for testing.
    pub(crate) async fn mutate_cache_for_test<F>(&self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut ModelsCache),
    {
        let mut cache = match self.load().await? {
            Some(cache) => cache,
            None => return Err(io::Error::new(ErrorKind::NotFound, "cache not found")),
        };
        f(&mut cache);
        self.save_internal(&cache).await
    }
}

async fn load_cache_file(path: &Path) -> io::Result<Option<ModelsCache>> {
    match fs::read(path).await {
        Ok(contents) => {
            let cache = serde_json::from_slice(&contents)
                .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
            Ok(Some(cache))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Serialized snapshot of models and metadata cached on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelsCache {
    pub(crate) fetched_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provider_id: Option<String>,
    pub(crate) models: Vec<ModelInfo>,
}

fn cache_version_matches(cached_version: Option<&str>, expected_version: &str) -> bool {
    match cached_version {
        Some(cached_version) if cached_version == expected_version => true,
        Some("0.0.0") if expected_version != "0.0.0" => true,
        _ => false,
    }
}

fn cache_provider_matches(cached_provider_id: Option<&str>, expected_provider_id: &str) -> bool {
    match cached_provider_id {
        Some(cached_provider_id) => cached_provider_id == expected_provider_id,
        None => expected_provider_id == OPENAI_PROVIDER_ID,
    }
}

impl ModelsCache {
    /// Returns `true` when the cache entry has not exceeded the configured TTL.
    fn is_fresh(&self, ttl: Duration) -> bool {
        if ttl.is_zero() {
            return false;
        }
        let Ok(ttl_duration) = chrono::Duration::from_std(ttl) else {
            return false;
        };
        let age = Utc::now().signed_duration_since(self.fetched_at);
        age <= ttl_duration
    }
}

#[cfg(test)]
mod tests {
    use super::cache_provider_matches;
    use super::cache_version_matches;

    #[test]
    fn cache_version_matches_accepts_exact_and_legacy_placeholder_versions() {
        assert!(cache_version_matches(Some("0.118.0"), "0.118.0"));
        assert!(cache_version_matches(Some("0.0.0"), "0.118.0"));
        assert!(!cache_version_matches(Some("0.117.0"), "0.118.0"));
        assert!(!cache_version_matches(None, "0.118.0"));
    }

    #[test]
    fn cache_provider_matches_treats_providerless_cache_as_openai_only() {
        assert!(cache_provider_matches(Some("openai"), "openai"));
        assert!(cache_provider_matches(None, "openai"));
        assert!(!cache_provider_matches(None, "github-copilot"));
        assert!(!cache_provider_matches(Some("openai"), "github-copilot"));
    }
}
