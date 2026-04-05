use bytes::Bytes;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use url::Url;

#[derive(Clone)]
pub struct CachedPackage {
    pub body: Bytes,
    pub etag: Option<String>,
}

#[derive(Debug, Error)]
pub enum EdgeFetchError {
    #[error("origin returned status {0}")]
    OriginStatus(u16),
    #[error("origin request failed: {0}")]
    OriginTransport(#[from] reqwest::Error),
}

/// Edge-side TTL cache with origin backfill on miss.
#[derive(Clone)]
pub struct EdgePackageCache {
    cache: Cache<String, Arc<CachedPackage>>,
    client: reqwest::Client,
    origin_base: Url,
}

impl EdgePackageCache {
    pub fn new(origin_base: Url, max_entries: u64, default_ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_entries)
            .time_to_live(default_ttl)
            .build();
        Self {
            cache,
            client: reqwest::Client::new(),
            origin_base,
        }
    }

    /// Returns cached bytes or fetches from origin, populating the cache.
    pub async fn get_or_fetch(&self, key: &str) -> Result<(Arc<CachedPackage>, bool), EdgeFetchError> {
        if let Some(hit) = self.cache.get(key).await {
            return Ok((hit, true));
        }

        let mut url = self.origin_base.clone();
        url.set_path(&format!("/packages/{}", key));

        let resp = self.client.get(url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(EdgeFetchError::OriginStatus(status.as_u16()));
        }

        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = resp.bytes().await?;
        let packaged = Arc::new(CachedPackage { body, etag });

        self.cache.insert(key.to_string(), packaged.clone()).await;

        Ok((packaged, false))
    }
}
