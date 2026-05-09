use std::path::PathBuf;

use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::error::Error;
use crate::session::Session;

pub type Result<T> = std::result::Result<T, Error>;

/// A single page of paginated results.
#[derive(Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_url: Option<String>,
}

/// Canvas API client with cookie injection, CSRF handling, and 401 auto-retry.
///
/// The session lives behind a `RwLock<SessionState>`. State carries a
/// monotonically-increasing generation counter so concurrent 401 callers can
/// tell whether someone else already re-logged-in (single-flight). The
/// session is `Option` so the client can be constructed BEFORE any session
/// exists — useful for long-running servers (cool-mcp) that want to start
/// up cleanly on a fresh machine and recover via the 401 path on the first
/// request.
pub struct CoolClient {
    http: reqwest::Client,
    state: RwLock<SessionState>,
    session_path: PathBuf,
    relogin_lock: Mutex<()>,
}

struct SessionState {
    session: Option<Session>,
    /// Bumped on every successful re-login. Used to detect "someone else
    /// already refreshed" inside `try_relogin_if_stale`.
    gen: u64,
}

impl CoolClient {
    pub fn new(session: Session, session_path: PathBuf) -> Self {
        Self::from_state(Some(session), session_path)
    }

    fn from_state(session: Option<Session>, session_path: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("cool-api/0.1.0")
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            state: RwLock::new(SessionState { session, gen: 0 }),
            session_path,
            relogin_lock: Mutex::new(()),
        }
    }

    pub fn from_session_path(path: PathBuf) -> Result<Self> {
        let session = Session::load(&path)?;
        Ok(Self::new(session, path))
    }

    pub fn from_default_session() -> Result<Self> {
        let path = Session::default_path();
        Self::from_session_path(path)
    }

    /// Tolerant constructor: succeeds even when no session.json exists.
    /// The first authenticated request will trigger `login_with_saved_credentials`
    /// (provided credentials.json is set up) via the 401 chain.
    pub fn from_default_session_lazy() -> Self {
        let path = Session::default_path();
        let session = Session::load(&path).ok();
        Self::from_state(session, path)
    }

    fn build_url(&self, session: &Session, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", session.base_url, path)
        }
    }

    fn default_headers(session: &Session) -> HeaderMap {
        let mut headers = HeaderMap::new();

        if let Some(csrf) = session.cookies.get("_csrf_token") {
            let decoded = urlencoding::decode(csrf).unwrap_or_else(|_| csrf.into());
            if let Ok(val) = HeaderValue::from_str(&decoded) {
                headers.insert("X-CSRF-Token", val);
            }
        }

        headers.insert(
            "X-Requested-With",
            HeaderValue::from_static("XMLHttpRequest"),
        );

        headers
    }

    fn cookie_header(session: &Session) -> String {
        session
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn request_with_session(
        &self,
        method: reqwest::Method,
        url: &str,
        session: &Session,
    ) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .headers(Self::default_headers(session))
            .header("Cookie", Self::cookie_header(session))
    }

    /// Snapshot the session for a single request. If no session exists yet,
    /// triggers a relogin via the same single-flight path 401 uses.
    async fn snapshot(&self) -> Result<(Session, u64)> {
        {
            let s = self.state.read().await;
            if let Some(ref sess) = s.session {
                return Ok((sess.clone(), s.gen));
            }
        }
        // No session yet — treat as a "permanent 401" and run the recovery chain.
        self.try_relogin_if_stale(0).await?;
        let s = self.state.read().await;
        let sess = s
            .session
            .clone()
            .ok_or_else(|| Error::Auth("relogin succeeded but session is empty".into()))?;
        Ok((sess, s.gen))
    }

    /// Re-login with saved credentials, but only if the current generation
    /// is still `observed_gen`. Concurrent 401 callers will all attempt this
    /// in series; the second-onward see a higher `gen` after the first
    /// completes and return `Ok(())` without re-running saml_login.
    async fn try_relogin_if_stale(&self, observed_gen: u64) -> Result<()> {
        let _guard = self.relogin_lock.lock().await;
        {
            let s = self.state.read().await;
            if s.gen > observed_gen && s.session.is_some() {
                return Ok(());
            }
        }
        let new_session = crate::auth::login_with_saved_credentials().await?;
        new_session.save(&self.session_path)?;
        let mut s = self.state.write().await;
        s.session = Some(new_session);
        s.gen = s.gen.saturating_add(1);
        Ok(())
    }

    fn is_unauthorized(err: &Error) -> bool {
        if let Error::Http(e) = err {
            return e.status() == Some(reqwest::StatusCode::UNAUTHORIZED);
        }
        false
    }

    // ----- Base HTTP methods (with auto-retry on 401) -----

    pub async fn get<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: Option<&Q>,
    ) -> Result<T> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<T> = async {
            let url = self.build_url(&session, path);
            let mut req = self.request_with_session(reqwest::Method::GET, &url, &session);
            if let Some(q) = query {
                req = req.query(q);
            }
            let resp = req.send().await?.error_for_status()?;
            Ok(resp.json().await?)
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let mut req = self.request_with_session(reqwest::Method::GET, &url, &session);
        if let Some(q) = query {
            req = req.query(q);
        }
        let resp = req.send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<T> = async {
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::POST, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(resp.json().await?)
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let resp = self
            .request_with_session(reqwest::Method::POST, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<T> = async {
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::PUT, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(resp.json().await?)
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let resp = self
            .request_with_session(reqwest::Method::PUT, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<T> = async {
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::DELETE, &url, &session)
                .send()
                .await?
                .error_for_status()?;
            Ok(resp.json().await?)
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let resp = self
            .request_with_session(reqwest::Method::DELETE, &url, &session)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<T> = async {
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::PATCH, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(resp.json().await?)
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let resp = self
            .request_with_session(reqwest::Method::PATCH, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    // ----- Void variants -----

    pub async fn get_void(&self, path: &str) -> Result<()> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<()> = async {
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::GET, &url, &session)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::GET, &url, &session)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn post_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<()> = async {
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::POST, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::POST, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn put_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<()> = async {
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::PUT, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::PUT, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn delete_void(&self, path: &str) -> Result<()> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<()> = async {
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::DELETE, &url, &session)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::DELETE, &url, &session)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn patch_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<()> = async {
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::PATCH, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::PATCH, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // ----- Pagination -----

    /// Fetch a single page and parse the Link header for the next URL.
    pub async fn get_paginated<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: Option<&Q>,
    ) -> Result<PaginatedResponse<T>> {
        let (session, gen) = self.snapshot().await?;
        let result: Result<PaginatedResponse<T>> = async {
            let url = self.build_url(&session, path);
            let mut req = self.request_with_session(reqwest::Method::GET, &url, &session);
            if let Some(q) = query {
                req = req.query(q);
            }
            let resp = req.send().await?.error_for_status()?;
            let next_url = parse_link_next(resp.headers());
            let items: Vec<T> = resp.json().await?;
            Ok(PaginatedResponse { items, next_url })
        }
        .await;

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin_if_stale(gen).await?;
        let (session, _) = self.snapshot().await?;
        let url = self.build_url(&session, path);
        let mut req = self.request_with_session(reqwest::Method::GET, &url, &session);
        if let Some(q) = query {
            req = req.query(q);
        }
        let resp = req.send().await?.error_for_status()?;
        let next_url = parse_link_next(resp.headers());
        let items: Vec<T> = resp.json().await?;
        Ok(PaginatedResponse { items, next_url })
    }

    // ----- Session access -----

    /// Returns the current session, if any. Used by call sites that need to
    /// know e.g. age (`Session::age_hours`) for diagnostics. Returns `None`
    /// when the client was constructed lazily and no request has yet
    /// triggered a login.
    pub async fn session(&self) -> Option<Session> {
        self.state.read().await.session.clone()
    }
}

/// Parse `Link` header to find `rel="next"` URL.
fn parse_link_next(headers: &HeaderMap) -> Option<String> {
    let link = headers.get("link")?.to_str().ok()?;

    for part in link.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") {
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            return Some(part[start..end].to_string());
        }
    }

    None
}
