use bytes::Bytes;
use std::collections::HashMap;
use std::sync::RwLock;

/// Authoritative in-memory store for the origin tier (canonical packages).
#[derive(Default)]
pub struct OriginStore {
    inner: RwLock<HashMap<String, Bytes>>,
}

impl OriginStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, key: impl Into<String>, body: Bytes) {
        let mut g = self.inner.write().expect("origin store lock poisoned");
        g.insert(key.into(), body);
    }

    pub fn get(&self, key: &str) -> Option<Bytes> {
        self.inner.read().expect("origin store lock poisoned").get(key).cloned()
    }

    pub fn list_keys(&self) -> Vec<String> {
        self.inner
            .read()
            .expect("origin store lock poisoned")
            .keys()
            .cloned()
            .collect()
    }
}
