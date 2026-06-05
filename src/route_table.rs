use std::sync::Arc;
use dashmap::DashMap;

#[derive(Debug, Clone)]
pub struct UpstreamTarget {
    pub upstream_url: String,
    pub enabled: bool,
}

/// Concurrent map of path prefix -> upstream target.
/// Longest-prefix match is performed in [`RouteTable::resolve`].
#[derive(Debug, Clone, Default)]
pub struct RouteTable {
    inner: Arc<DashMap<String, UpstreamTarget>>,
}

impl RouteTable {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn upsert(&self, prefix: impl Into<String>, target: UpstreamTarget) {
        self.inner.insert(prefix.into(), target);
    }

    pub fn remove(&self, prefix: &str) {
        self.inner.remove(prefix);
    }

    /// Returns the upstream target and the matched prefix for a given path,
    /// using longest-prefix matching. Only returns enabled targets.
    pub fn resolve(&self, path: &str) -> Option<(String, UpstreamTarget)> {
        let mut best: Option<(String, UpstreamTarget)> = None;
        for entry in self.inner.iter() {
            let prefix = entry.key();
            let target = entry.value();
            if !target.enabled {
                continue;
            }
            if path.starts_with(prefix.as_str()) {
                let is_longer = best
                    .as_ref()
                    .map(|(p, _)| prefix.len() > p.len())
                    .unwrap_or(true);
                if is_longer {
                    best = Some((prefix.clone(), target.clone()));
                }
            }
        }
        best
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn clear(&self) {
        self.inner.clear();
    }

    /// Remove all remote routes (those starting with /remotes/).
    pub fn clear_remotes(&self) {
        self.inner.retain(|k, _| !k.starts_with("/remotes/"));
    }

    /// Return all (prefix, target) pairs — used when copying routes between tables.
    pub fn iter_all(&self) -> Vec<(String, UpstreamTarget)> {
        self.inner
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }
}
