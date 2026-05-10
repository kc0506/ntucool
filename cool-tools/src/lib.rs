//! cool-tools — Reusable tool surface for NTU COOL.
//!
//! The contract layer between `cool-api` (Canvas client) and frontends
//! (`cool-cli`, `cool-mcp`). Pure logic, plain structs in/out — no IO
//! formatting, no progress bars, no `println!`.

pub mod announcements;
pub mod assignments;
pub mod attachments;
pub mod courses;
pub mod discussions;
pub mod files;
pub mod modules;
pub mod pages;
pub mod pdf;
pub mod profile;
pub mod text;
pub mod types;
pub mod users;

pub use cool_api::CoolClient;
