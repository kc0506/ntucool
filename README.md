# ntucool

> [!WARNING]
> This is an **unofficial** project. Use it at your own risk, and responsibly.

NTU COOL (cool.ntu.edu.tw) CLI + MCP server.

It also includes an NTU COOL SDK — currently Rust only, with Python and JavaScript / TypeScript planned.

NTU-only: login goes through NTU's ADFS SAML flow. The interfaces follow Canvas LMS but are adapted to NTU's Canvas instance.

## Install

ntucool is two binaries — `cool` (CLI) and `ntucool-mcp` (MCP server). The Claude Code plugin is a thin layer that installs and drives them; if you use Claude Code, that's the only section you need.

### Claude Code Quickstart

This repo contains a Claude Code plugin — a usage skill for AI agents, the `/ntucool:setup` command, and the MCP server config.

```
/plugin marketplace add kc0506/ntucool
/plugin install ntucool@ntucool
```

Then run `/ntucool:setup` — it installs the `cool` and `ntucool-mcp` binaries and walks you through login. After that the MCP tools and the `cool` CLI both work; you can skip the rest of this section.

### CLI, or another MCP client

For a plain terminal CLI, or to wire `ntucool-mcp` into Claude Desktop / Cursor, install the binaries directly:

<details>
<summary>Linux / Windows — prebuilt binaries (no Rust needed)</summary>

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.sh | sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.ps1 | iex"
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.ps1 | iex"
```
</details>

<details>
<summary>macOS, or building from source</summary>

```sh
cargo install ntucool ntucool-mcp
```

macOS prebuilt binaries aren't available yet — install from source with `cargo` for now.
</details>

Then log in once:

```sh
cool login
```

## CLI

```sh
cool whoami
cool course list
cool grade
cool submission mine --status graded
cool file list --course 57439 --path /
```

`cool --help` for the full command surface.

<details>
<summary>TUI (work in progress)</summary>

Some commands also ship an interactive TUI — e.g. `cool assignment`.
</details>

### Supported CLI commands

| Command | Purpose |
|---|---|
| `cool login` | Interactive ADFS login + save credentials. **CLI-only**. |
| `cool logout [--purge]` | Drop session cookies; `--purge` also deletes credentials.json. **CLI-only**. |
| `cool whoami` | Self profile (includes `login_id` + `primary_email`). |
| `cool course list [--all] [--term <id>]` | List enrolments. |
| `cool course show <course_id>` | Course detail (syllabus, term, teachers). |
| `cool assignment list/info` | Assignment list / detail (see `cool assignment --help`). |
| `cool assignment submit <files…\|--text> -c <course> -a <id>` | Submit an assignment. Prints a preflight + risk summary; confirms interactively (`--i-understand` skips). **Irreversible — creates a graded attempt.** |
| `cool announcement list/show` | Announcements per course. |
| `cool discussion list/show` | Discussion topics. |
| `cool module list/show` | Modules; `list` returns items inline. |
| `cool module download <course> <module>` | Bulk file download with multi-progress bars. **CLI-only**. |
| `cool file ls/download/upload` | File ops. **`upload` is CLI-only**. |
| `cool user get <user_id>` | Look up another user (name + avatar; no email at student privilege). |
| `cool submission mine [--course] [--status]` | Self submissions across one / all active courses. |
| `cool grade [--course <id>]` | Per-course grade summary. |

## MCP

`ntucool-mcp` is a stdio MCP server. Claude Desktop / Cursor config:

```json
{
  "mcpServers": {
    "ntucool": {
      "command": "ntucool-mcp"
    }
  }
}
```

### Available MCP tools

| Domain | Tools |
|---|---|
| Identity | `whoami`, `users_get` |
| Courses | `courses_list`, `courses_resolve`, `courses_get` |
| Files | `files_list`, `files_search`, `files_get_metadata`, `files_fetch` |
| Assignments | `assignments_list`, `assignments_get`, `assignments_submit` |
| Announcements | `announcements_list`, `announcements_get` |
| Modules | `modules_list`, `modules_get` |
| Discussions | `discussions_list`, `discussions_get` |
| Pages | `pages_list`, `pages_get` |
| PDFs | `pdf_search`, `pdf_extract` |
| Self grading | `submissions_mine`, `grades_get` |

## Reference

`docs/TOOLS.md` and `plugins/ntucool/skills/ntucool/SKILL.md` carry the full per-tool contract — more comprehensive than this page, but noisier.

## Security

- **Credentials.** `cool login` saves a session cookie under `$XDG_DATA_HOME/ntucool/` and, if you opt in, credentials under `$XDG_CONFIG_HOME/ntucool/credentials.json` (mode `0600`). You choose how the password is stored — plaintext, a `password_cmd` (`pass`, `op`, ...), or not saved at all. Nothing is sent anywhere except NTU COOL itself.
- **Write operations.** Most tools are read-only. The exceptions — `assignment submit` and `file upload` — change state on NTU COOL. `submit` is irreversible (it creates a graded attempt) and prints a preflight summary before proceeding.
- **Abuse.** Please don't abuse the tools this project provides.

## NTU COOL SDK

Under development. See `codegen/` for the current pipeline and `cool-api/` for the generated Rust client.

## How it works

- NTU COOL is built on Canvas LMS, an open-source platform.
- We collect Canvas LMS API schemas from official sources.
- `codegen/` transforms them into a per-language SDK.
- Both the CLI and the MCP server use the generated Rust client for API requests.

### Authentication

NTU COOL does not support Canvas's native API tokens. We therefore emulate NTU's SAML login and extract the session cookies — which currently means typing your student account and password into `cool login`. The cookies are saved to `$XDG_DATA_HOME/ntucool/session.json` and attached to every API request. They expire after ~24h on NTU COOL's side; the next call then transparently re-logs in from saved credentials, or prompts if none were saved.

## License

MIT.
