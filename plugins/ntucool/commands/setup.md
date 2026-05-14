---
description: Install ntucool binaries and log in to NTU COOL
allowed-tools: Bash, AskUserQuestion
---

# ntucool setup

End state: `cool` and `ntucool-mcp` on PATH, a valid NTU COOL session on disk, `cool whoami` works. Once that's true, the MCP tools and CLI both work end-to-end.

## Step 0: Detect current state

```bash
which cool ntucool-mcp 2>/dev/null
[ -f "${XDG_DATA_HOME:-$HOME/.local/share}/ntucool/session.json" ] && echo "session: present" || echo "session: missing"
cool whoami 2>&1 | head -1
```

Decide what's needed:

| Both binaries present? | `cool whoami` succeeds? | Action |
|---|---|---|
| no | — | Step 1 (install) → Step 2 (login) → Step 3 (verify) |
| yes | yes | Already set up. Stop. |
| yes | no | Step 2 (login) → Step 3 (verify) |

## Step 1: Install binaries

Branch on the environment context's `Platform:` value (do not run `uname`; the Bash tool may report MINGW/MSYS under Windows).

**Linux** (`linux`) — also covers WSL:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.sh | sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.sh | sh
```

**Windows + bash / Git Bash** (`win32`, Shell `bash`): same `curl` commands as Linux.

**Windows + PowerShell** (`win32`, Shell `powershell` / `pwsh` / `cmd`):

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.ps1 | iex"
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.ps1 | iex"
```

**macOS** (`darwin`): no prebuilt binaries yet (GitHub free-tier macOS runner queue is unresolved). Fall back to `cargo install`:

```bash
command -v cargo 2>&1
```

If `cargo` is found:
```bash
cargo install ntucool ntucool-mcp
```

If not, stop and tell the user: install Rust from <https://rustup.rs/>, then re-run `/ntucool:setup`.

After install, confirm both binaries resolve:
```bash
which cool ntucool-mcp
```

Both should print a path under `~/.cargo/bin/` (or the matching Windows location). If either is empty, stop and surface the install error — don't proceed to Step 2.

## Step 2: Log in

`cool login` is interactive — it prompts for NTU username, password, and asks how to save credentials for non-interactive re-login (plaintext / `password_cmd` / never). The Bash tool can't drive interactive stdin; the user must type the command themselves.

Tell the user verbatim:

> Run `! cool login` and complete the prompts. Pick "Yes — store password in credentials.json (mode 0600)" or "Yes — configure a password_cmd" so the MCP server can re-login automatically when the session expires (every ~24h).

Wait for them to finish before continuing.

## Step 3: Verify

```bash
cool whoami
```

Should print the user's display name, `login_id`, and `primary_email`. If yes — setup is complete. Tell the user:

> Setup is complete. The MCP server reads the same session/credentials, so the ntucool tools will work in this conversation. If Claude Code already tried to start `ntucool-mcp` before the binaries were installed and cached a failure, restart Claude Code once.

If `cool whoami` errors, map by error:

- `command not found: cool` → Step 1 didn't install. Re-check platform branch.
- `401`, `not logged in`, `session expired and re-login failed` → Step 2 incomplete or credentials weren't saved. Re-run Step 2.
- network error → ask user to check connectivity to `cool.ntu.edu.tw`.

## Notes

- Binaries land in `$CARGO_HOME/bin` (typically `~/.cargo/bin/`). Make sure that's on `PATH` — the installer prints a warning if it isn't.
- Session lives at `$XDG_DATA_HOME/ntucool/session.json` (Linux/macOS) or `%APPDATA%\ntucool\session.json` (Windows).
- Credentials (optional, written by Step 2 if you chose to save them) live at `$XDG_CONFIG_HOME/ntucool/credentials.json` with `0600` permissions on Unix.
- To reset everything: `cool logout --purge` then `/ntucool:setup`.
