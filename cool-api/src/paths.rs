//! Canonical filesystem paths.
//!
//! Single source of truth for every XDG-derived location the project uses.
//! Hand-rolled `env::var("XDG_*")` blocks elsewhere are migrating here so
//! that the resolution logic and the `~/.config` / `~/.local/share` /
//! `~/.cache` fallbacks stay in lockstep across crates and platforms.

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .expect("HOME not set")
}

pub fn xdg_config_home() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".config"))
}

pub fn xdg_data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".local").join("share"))
}

pub fn xdg_cache_home() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".cache"))
}

// ─── ntucool (cool-api / cool-cli / cool-tui) ───────────────────────────────

pub fn credentials_path() -> PathBuf {
    xdg_config_home().join("ntucool").join("credentials.json")
}

pub fn session_path() -> PathBuf {
    xdg_data_home().join("ntucool").join("session.json")
}

pub fn courses_cache_path() -> PathBuf {
    xdg_cache_home().join("ntucool").join("courses.json")
}

pub fn all_enrolments_cache_path() -> PathBuf {
    xdg_cache_home().join("ntucool").join("courses-all.json")
}

// ─── cool-mcp ───────────────────────────────────────────────────────────────

pub fn mcp_files_cache_dir() -> PathBuf {
    xdg_cache_home().join("cool-mcp").join("cache")
}

pub fn mcp_text_cache_dir() -> PathBuf {
    xdg_cache_home().join("cool-mcp").join("text")
}

pub fn mcp_publish_dir() -> PathBuf {
    xdg_data_home().join("cool-mcp").join("files")
}
