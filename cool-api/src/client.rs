use std::path::PathBuf;

use reqwest::header::{HeaderMap, HeaderValue};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::error::Error;
use crate::session::Session;

pub type Result<T> = std::result::Result<T, Error>;

/// A single page of paginated results.
#[derive(Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_url: Option<String>,
}

/// Canvas API client with automatic cookie injection, CSRF handling, and 401 auto-retry.
pub struct CoolClient {
    http: reqwest::Client,
    session: RwLock<Session>,
    session_path: PathBuf,
}

impl CoolClient {
    pub fn new(session: Session, session_path: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("cool-api/0.1.0")
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            session: RwLock::new(session),
            session_path,
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

    /// Attempt re-login with saved credentials and update internal session.
    /// Returns Ok(()) if successful, or the original error if re-login fails.
    async fn try_relogin(&self) -> Result<()> {
        let new_session = crate::auth::login_with_saved_credentials().await?;
        new_session.save(&self.session_path)?;
        *self.session.write().await = new_session;
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
        // First attempt
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            let mut req = self.request_with_session(reqwest::Method::GET, &url, &session);
            if let Some(q) = query {
                req = req.query(q);
            }
            let resp = req.send().await?.error_for_status()?;
            resp.json().await.map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        // 401 → retry after re-login
        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::POST, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            resp.json().await.map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::PUT, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            resp.json().await.map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::DELETE, &url, &session)
                .send()
                .await?
                .error_for_status()?;
            resp.json().await.map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            let resp = self
                .request_with_session(reqwest::Method::PATCH, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()?;
            resp.json().await.map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::GET, &url, &session)
                .send()
                .await?
                .error_for_status()
                .map(|_| ())
                .map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::GET, &url, &session)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn post_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::POST, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()
                .map(|_| ())
                .map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::POST, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn put_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::PUT, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()
                .map(|_| ())
                .map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::PUT, &url, &session)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn delete_void(&self, path: &str) -> Result<()> {
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::DELETE, &url, &session)
                .send()
                .await?
                .error_for_status()
                .map(|_| ())
                .map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
        let url = self.build_url(&session, path);
        self.request_with_session(reqwest::Method::DELETE, &url, &session)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn patch_void<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let result = {
            let session = self.session.read().await;
            let url = self.build_url(&session, path);
            self.request_with_session(reqwest::Method::PATCH, &url, &session)
                .json(body)
                .send()
                .await?
                .error_for_status()
                .map(|_| ())
                .map_err(Error::from)
        };

        match result {
            Err(ref e) if Self::is_unauthorized(e) => {}
            other => return other,
        }

        self.try_relogin().await?;
        let session = self.session.read().await;
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
        let result: Result<PaginatedResponse<T>> = async {
            let session = self.session.read().await;
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

        self.try_relogin().await?;
        let session = self.session.read().await;
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

    pub async fn session(&self) -> Session {
        self.session.read().await.clone()
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
