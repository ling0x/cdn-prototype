//! Edge-oriented package delivery: regional routing hints, TTL caching at the edge,
//! and origin backfill so clients get bytes quickly with a clear miss path.

pub mod edge_cache;
pub mod origin_store;
pub mod routing;
pub mod types;

pub use edge_cache::{EdgeFetchError, EdgePackageCache};
pub use origin_store::OriginStore;
pub use routing::{EdgeDirectory, Region};
pub use types::PackageKey;
