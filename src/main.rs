use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use cdn_prototype::{EdgeDirectory, EdgeFetchError, EdgePackageCache, OriginStore, Region};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use url::Url;

#[derive(Parser)]
#[command(name = "cdn-prototype", about = "Edge cache demo: origin + edge + regional routing hints")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the authoritative origin (packages are stored here).
    Origin {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: String,
    },
    /// Run an edge node that caches packages from `--origin`.
    Edge {
        #[arg(long, default_value = "127.0.0.1:8081")]
        listen: String,
        #[arg(long)]
        origin: String,
        #[arg(long, default_value_t = 300)]
        default_ttl_secs: u64,
        #[arg(long, default_value_t = 10_000)]
        max_entries: u64,
    },
    /// Print ordered edge base URLs for a region (demo directory).
    Resolve {
        #[arg(value_enum)]
        region: RegionArg,
    },
}

#[derive(Clone, ValueEnum)]
enum RegionArg {
    Americas,
    Europe,
    AsiaPacific,
    Global,
}

impl From<RegionArg> for Region {
    fn from(value: RegionArg) -> Self {
        match value {
            RegionArg::Americas => Region::Americas,
            RegionArg::Europe => Region::Europe,
            RegionArg::AsiaPacific => Region::AsiaPacific,
            RegionArg::Global => Region::Global,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cdn_prototype=info,tower_http=info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Origin { listen } => run_origin(listen).await?,
        Command::Edge {
            listen,
            origin,
            default_ttl_secs,
            max_entries,
        } => run_edge(listen, origin, default_ttl_secs, max_entries).await?,
        Command::Resolve { region } => {
            let dir = EdgeDirectory::demo();
            let urls = dir.resolve(region.into());
            println!("{}", serde_json::to_string_pretty(&urls)?);
        }
    }
    Ok(())
}

async fn run_origin(listen: String) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = listen.parse()?;
    let store = Arc::new(OriginStore::new());
    // Tiny seed so a cold edge has something to pull.
    store.put(
        "hello",
        Bytes::from_static(b"hello from origin - seed packages with PUT /packages/:key\n"),
    );

    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route(
            "/packages/{key}",
            get(origin_get).put(origin_put),
        )
        .with_state(store)
        .layer(TraceLayer::new_for_http());

    info!(%addr, "origin listening");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn origin_get(
    State(store): State<Arc<OriginStore>>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let Some(body) = store.get(&key) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let etag = weak_etag(&body);
    let cache_control = "public, max-age=3600";

    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(axum::http::header::ETAG, etag)
        .header(axum::http::header::CACHE_CONTROL, cache_control)
        .body(Body::from(body))
        .unwrap()
}

async fn origin_put(
    State(store): State<Arc<OriginStore>>,
    Path(key): Path<String>,
    body: Bytes,
) -> StatusCode {
    if key.is_empty() || key.contains('/') {
        return StatusCode::BAD_REQUEST;
    }
    store.put(key, body);
    StatusCode::NO_CONTENT
}

fn weak_etag(body: &Bytes) -> HeaderValue {
    let mut h = DefaultHasher::new();
    body.as_ref().hash(&mut h);
    let token = format!("W/\"{:x}\"", h.finish());
    HeaderValue::try_from(token).unwrap()
}

async fn run_edge(
    listen: String,
    origin: String,
    default_ttl_secs: u64,
    max_entries: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = listen.parse()?;
    let origin_base: Url = origin.parse()?;
    let cache = Arc::new(EdgePackageCache::new(
        origin_base,
        max_entries,
        std::time::Duration::from_secs(default_ttl_secs),
    ));

    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/packages/{key}", get(edge_get))
        .with_state(cache)
        .layer(TraceLayer::new_for_http());

    info!(%addr, "edge listening");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn edge_get(
    State(cache): State<Arc<EdgePackageCache>>,
    Path(key): Path<String>,
) -> Result<Response, EdgeHttpError> {
    if key.is_empty() || key.contains('/') {
        return Err(EdgeHttpError::BadKey);
    }
    let (pkg, hit) = cache.get_or_fetch(&key).await?;

    let mut res = Response::new(Body::from(pkg.body.clone()));
    res.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    res.headers_mut().insert(
        axum::http::HeaderName::from_static("x-cache"),
        HeaderValue::from_static(if hit { "HIT" } else { "MISS" }),
    );
    if let Some(etag) = &pkg.etag {
        if let Ok(v) = HeaderValue::from_str(etag) {
            res.headers_mut().insert(axum::http::header::ETAG, v);
        }
    }
    Ok(res)
}

#[derive(Debug)]
enum EdgeHttpError {
    BadKey,
    Fetch(EdgeFetchError),
}

impl From<EdgeFetchError> for EdgeHttpError {
    fn from(value: EdgeFetchError) -> Self {
        Self::Fetch(value)
    }
}

impl IntoResponse for EdgeHttpError {
    fn into_response(self) -> Response {
        match self {
            EdgeHttpError::BadKey => StatusCode::BAD_REQUEST.into_response(),
            EdgeHttpError::Fetch(EdgeFetchError::OriginStatus(404)) => {
                StatusCode::NOT_FOUND.into_response()
            }
            EdgeHttpError::Fetch(EdgeFetchError::OriginStatus(s)) if (400..500).contains(&s) => {
                StatusCode::BAD_GATEWAY.into_response()
            }
            EdgeHttpError::Fetch(_) => StatusCode::BAD_GATEWAY.into_response(),
        }
    }
}
