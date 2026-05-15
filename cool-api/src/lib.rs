// cool-api: NTU COOL Canvas API client for Rust

pub mod auth;
pub mod client;
pub mod config;
pub mod credentials;
pub mod download;
pub mod error;
pub mod paths;
pub mod session;
pub mod upload;

pub mod generated {
    pub mod models;
    pub mod params;
    pub mod endpoints;
}

pub use client::CoolClient;
pub use credentials::Credentials;
pub use error::Error;
pub use session::Session;
