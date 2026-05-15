//! Project-scoped configuration — currently just the write-permission level.
//!
//! Resolution order, highest priority first:
//!   1. env `NTUCOOL_WRITE_LEVEL` — for one-off CLI runs with no config file
//!   2. nearest `.ntucool.json` walking up from the current directory
//!   3. built-in default (`WriteLevel::None`)
//!
//! Both `cool-cli` and `cool-mcp` resolve through here, so the write policy is
//! identical regardless of which frontend issues the call. Note this is a
//! convenience/safety gate, not a security boundary — anything that can edit
//! files or set env vars can change it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How much the submit (write) path is allowed to do. Ordinal — each level is
/// a strict superset of the previous one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteLevel {
    /// No writes at all. Submission is refused outright. (default)
    #[default]
    None,
    /// Writes allowed, but *any* flagged risk aborts — `i_understand` has no
    /// power. Only fully clean submissions go through.
    Safe,
    /// Writes allowed; "danger" risks (past due, re-submission) proceed when
    /// `i_understand` is set; "will-fail" risks still abort.
    Guarded,
    /// Writes allowed, every preflight check skipped — Canvas is the only
    /// authority. Caller takes full responsibility.
    Unguarded,
}

impl WriteLevel {
    /// Parse a level from a string. Accepts the canonical names and the
    /// ordinals `0`-`3` as aliases. Case-insensitive.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "0" => Some(Self::None),
            "safe" | "1" => Some(Self::Safe),
            "guarded" | "2" => Some(Self::Guarded),
            "unguarded" | "3" => Some(Self::Unguarded),
            _ => None,
        }
    }

    /// Canonical name, as written in `.ntucool.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Safe => "safe",
            Self::Guarded => "guarded",
            Self::Unguarded => "unguarded",
        }
    }
}

/// On-disk shape of `.ntucool.json`. Every field defaults, so an empty `{}` —
/// or a file with unrelated future keys — still parses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub write_level: WriteLevel,
}

/// Filename looked up when walking parent directories.
pub const CONFIG_FILENAME: &str = ".ntucool.json";

/// Walk up from `start` looking for `.ntucool.json`; return the first hit.
fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Load the nearest `.ntucool.json` (walking up from the current directory),
/// or defaults when none exists. A malformed file fails *closed* — it warns on
/// stderr and falls back to the safe default rather than guessing.
pub fn load() -> Config {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let Some(path) = find_config_file(&cwd) else {
        return Config::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "ntucool: ignoring malformed {} ({e}) — treating write_level as `none`",
                    path.display()
                );
                Config::default()
            }
        },
        Err(e) => {
            eprintln!("ntucool: cannot read {} ({e})", path.display());
            Config::default()
        }
    }
}

/// The effective write level: `NTUCOOL_WRITE_LEVEL` if set and valid, else the
/// nearest config file's value, else `WriteLevel::None`.
pub fn write_level() -> WriteLevel {
    if let Ok(raw) = std::env::var("NTUCOOL_WRITE_LEVEL") {
        if let Some(level) = WriteLevel::parse(&raw) {
            return level;
        }
        eprintln!(
            "ntucool: ignoring invalid NTUCOOL_WRITE_LEVEL={raw:?} \
             (expected none|safe|guarded|unguarded) — falling back to config file"
        );
    }
    load().write_level
}
