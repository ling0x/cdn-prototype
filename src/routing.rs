use std::collections::HashMap;

/// Coarse region bucket used to pick preferred edge endpoints (GeoDNS-style).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Region {
    Americas,
    Europe,
    AsiaPacific,
    /// Fallback when region is unknown or for global anycast-style edges.
    Global,
}

/// Maps each region to an ordered list of edge base URLs (first = preferred).
#[derive(Clone, Debug, Default)]
pub struct EdgeDirectory {
    edges_by_region: HashMap<Region, Vec<String>>,
}

impl EdgeDirectory {
    pub fn insert(&mut self, region: Region, edges: Vec<String>) {
        self.edges_by_region.insert(region, edges);
    }

    /// Preferred edges for `region`, then `Global`, then any other region's edges as last resort.
    pub fn resolve(&self, region: Region) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(v) = self.edges_by_region.get(&region) {
            out.extend(v.iter().cloned());
        }
        if region != Region::Global {
            if let Some(v) = self.edges_by_region.get(&Region::Global) {
                for u in v {
                    if !out.contains(u) {
                        out.push(u.clone());
                    }
                }
            }
        }
        if out.is_empty() {
            for v in self.edges_by_region.values() {
                for u in v {
                    if !out.contains(u) {
                        out.push(u.clone());
                    }
                }
            }
        }
        out
    }

    /// Demo topology: three regional edges + one global failover.
    pub fn demo() -> Self {
        let mut d = Self::default();
        d.insert(
            Region::Americas,
            vec![
                "https://edge-us-east.example.invalid".into(),
                "https://edge-us-west.example.invalid".into(),
            ],
        );
        d.insert(
            Region::Europe,
            vec![
                "https://edge-eu-central.example.invalid".into(),
                "https://edge-eu-west.example.invalid".into(),
            ],
        );
        d.insert(
            Region::AsiaPacific,
            vec![
                "https://edge-ap-south.example.invalid".into(),
                "https://edge-ap-northeast.example.invalid".into(),
            ],
        );
        d.insert(
            Region::Global,
            vec!["https://edge-global.example.invalid".into()],
        );
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_region_then_global() {
        let mut d = EdgeDirectory::default();
        d.insert(Region::Europe, vec!["https://eu.example".into()]);
        d.insert(Region::Global, vec!["https://global.example".into()]);
        let urls = d.resolve(Region::Europe);
        assert_eq!(urls[0], "https://eu.example");
        assert!(urls.contains(&"https://global.example".into()));
    }

    #[test]
    fn resolve_falls_back_when_region_empty() {
        let mut d = EdgeDirectory::default();
        d.insert(Region::Global, vec!["https://global.example".into()]);
        let urls = d.resolve(Region::Americas);
        assert_eq!(urls, vec!["https://global.example".to_string()]);
    }
}
