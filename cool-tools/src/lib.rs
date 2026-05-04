//! cool-tools — Reusable tool surface for NTU COOL.
//!
//! The contract layer between `cool-api` (Canvas client) and frontends
//! (`cool-cli`, `cool-mcp`). Pure logic, plain structs in/out — no IO
//! formatting, no progress bars, no `println!`.

pub mod announcements;
pub mod assignments;
pub mod courses;
pub mod discussions;
pub mod files;
pub mod modules;
pub mod profile;
pub mod text;

pub use cool_api::CoolClient;
