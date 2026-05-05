//! Public file publisher — turns a server-internal `CachedFile` into a URI
//! the AI client can read.
//!
//! Architecture: the server's internal cache (`cool_tools::files::cache_or_download`)
//! is a private optimisation; its on-disk paths must NEVER appear in any URI
//! exposed to the client. This module enforces that boundary.
//!
//! Two modes:
//!   * stdio — copy the cached bytes into a separate `output_dir` and return
//!     `file://<output_dir>/<id>/<display_name>`. Client and cache are decoupled
//!     even on the same machine; client can move or delete the output without
//!     poisoning the server cache.
//!   * http  — spawn a local HTTP server, mint an opaque time-limited token,
//!     return `http://host:port/files/<token>`. The token map is kept in the
//!     server process; the cache path is never exposed.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};

use cool_tools::files::CachedFile;
use cool_tools::types::FilesFetchResult;

// ────────────────────────────────────────────────────────────────────────────
// Public enum dispatched at runtime
// ────────────────────────────────────────────────────────────────────────────

pub enum FileServer {
    Stdio(StdioPublisher),
    Http(HttpPublisher),
}

impl FileServer {
    pub async fn publish(&self, cached: CachedFile) -> Result<FilesFetchResult> {
        match self {
            FileServer::Stdio(s) => s.publish(cached).await,
            FileServer::Http(h) => h.publish(cached).await,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            FileServer::Stdio(s) => format!("stdio (output_dir={})", s.output_dir().display()),
            FileServer::Http(h) => format!("http (public_base={}, ttl={}s)", h.public_base(), h.ttl().as_secs()),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// stdio publisher: copy → `file://`
// ────────────────────────────────────────────────────────────────────────────

pub struct StdioPublisher {
    output_dir: PathBuf,
}

impl StdioPublisher {
    pub fn new(output_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&output_dir)
            .with_context(|| format!("create output dir {}", output_dir.display()))?;
        Ok(Self { output_dir })
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    async fn publish(&self, cached: CachedFile) -> Result<FilesFetchResult> {
        let safe_name = sanitize_filename(&cached.display_name);
        let id_dir = self.output_dir.join(cached.file_id.to_string());
        tokio::fs::create_dir_all(&id_dir)
            .await
            .with_context(|| format!("create {}", id_dir.display()))?;
        let dest = id_dir.join(&safe_name);

        let needs_copy = match (
            tokio::fs::metadata(&dest).await,
            tokio::fs::metadata(&cached.path).await,
        ) {
            (Ok(d_meta), Ok(c_meta)) => {
                d_meta.len() != c_meta.len() || d_meta.modified().ok() < c_meta.modified().ok()
            }
            _ => true,
        };
        if needs_copy {
            tokio::fs::copy(&cached.path, &dest).await.with_context(|| {
                format!("copy {} → {}", cached.path.display(), dest.display())
            })?;
        }

        let abs = dest.canonicalize().unwrap_or(dest);
        Ok(FilesFetchResult {
            file_id: cached.file_id,
            display_name: cached.display_name,
            mime_type: cached.mime_type,
            size_bytes: cached.size_bytes,
            uri: format!("file://{}", abs.display()),
            expires_at: None,
        })
    }
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '\0' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

// ────────────────────────────────────────────────────────────────────────────
// http publisher: token-signed URL
// ────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TokenEntry {
    cache_path: PathBuf,
    display_name: String,
    mime_type: Option<String>,
    size_bytes: i64,
    expires_at: SystemTime,
}

#[derive(Clone)]
struct HttpState {
    tokens: Arc<Mutex<HashMap<String, TokenEntry>>>,
}

pub struct HttpPublisher {
    public_base: String,
    ttl: Duration,
    state: HttpState,
    by_cache: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl HttpPublisher {
    /// Bind to `bind_addr`, spawn the axum server in a background task,
    /// and return a publisher tied to it. `public_base_override` is used
    /// when fronted by a reverse proxy; otherwise we derive `http://addr`.
    pub async fn start(
        bind_addr: SocketAddr,
        public_base_override: Option<String>,
        ttl: Duration,
    ) -> Result<Self> {
        use axum::{routing::get, Router};

        let state = HttpState {
            tokens: Arc::new(Mutex::new(HashMap::new())),
        };

        let app = Router::new()
            .route("/files/{token}", get(serve_token))
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .with_context(|| format!("bind {bind_addr}"))?;
        let actual = listener.local_addr()?;

        let public_base = public_base_override.unwrap_or_else(|| format!("http://{actual}"));
        tracing::info!(%actual, %public_base, ttl_secs = ttl.as_secs(), "HTTP file server listening");

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!(error = %e, "axum server exited");
            }
        });

        Ok(Self {
            public_base,
            ttl,
            state,
            by_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn public_base(&self) -> &str {
        &self.public_base
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    async fn publish(&self, cached: CachedFile) -> Result<FilesFetchResult> {
        let now = SystemTime::now();

        // Idempotent reuse: same cache_path within TTL → same token.
        let existing = {
            let by_cache = self.by_cache.lock().unwrap();
            by_cache.get(&cached.path).cloned()
        };
        if let Some(token) = existing {
            let alive_exp = {
                let tokens = self.state.tokens.lock().unwrap();
                tokens
                    .get(&token)
                    .filter(|e| e.expires_at > now)
                    .map(|e| e.expires_at)
            };
            if let Some(exp) = alive_exp {
                return Ok(self.make_result(cached, &token, exp));
            }
        }

        let token = mint_token();
        let expires_at = now + self.ttl;
        let entry = TokenEntry {
            cache_path: cached.path.clone(),
            display_name: cached.display_name.clone(),
            mime_type: cached.mime_type.clone(),
            size_bytes: cached.size_bytes,
            expires_at,
        };
        self.state.tokens.lock().unwrap().insert(token.clone(), entry);
        self.by_cache
            .lock()
            .unwrap()
            .insert(cached.path.clone(), token.clone());

        Ok(self.make_result(cached, &token, expires_at))
    }

    fn make_result(&self, cached: CachedFile, token: &str, exp: SystemTime) -> FilesFetchResult {
        let exp_iso: chrono::DateTime<chrono::Utc> = exp.into();
        FilesFetchResult {
            file_id: cached.file_id,
            display_name: cached.display_name,
            mime_type: cached.mime_type,
            size_bytes: cached.size_bytes,
            uri: format!(
                "{}/files/{}",
                self.public_base.trim_end_matches('/'),
                token
            ),
            expires_at: Some(exp_iso.to_rfc3339()),
        }
    }
}

fn mint_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    let mut s = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

async fn serve_token(
    axum::extract::State(state): axum::extract::State<HttpState>,
    axum::extract::Path(token): axum::extract::Path<String>,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let now = SystemTime::now();
    let entry: Option<TokenEntry> = {
        let map = state.tokens.lock().unwrap();
        map.get(&token).cloned()
    };
    let entry = match entry {
        Some(e) if e.expires_at > now => e,
        Some(_) => return Err(axum::http::StatusCode::GONE),
        None => return Err(axum::http::StatusCode::NOT_FOUND),
    };

    let f = tokio::fs::File::open(&entry.cache_path)
        .await
        .map_err(|_| axum::http::StatusCode::NOT_FOUND)?;
    let stream = tokio_util::io::ReaderStream::new(f);
    let body = axum::body::Body::from_stream(stream);

    let mut resp = axum::response::Response::new(body);
    let h = resp.headers_mut();
    if let Some(mime) = &entry.mime_type {
        if let Ok(v) = mime.parse() {
            h.insert(axum::http::header::CONTENT_TYPE, v);
        }
    }
    let safe_disp = entry.display_name.replace('"', "_");
    if let Ok(v) = format!("attachment; filename=\"{safe_disp}\"").parse() {
        h.insert(axum::http::header::CONTENT_DISPOSITION, v);
    }
    if let Ok(v) = entry.size_bytes.to_string().parse() {
        h.insert(axum::http::header::CONTENT_LENGTH, v);
    }
    Ok(resp)
}
