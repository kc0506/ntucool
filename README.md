# ntucool

> warning: This is an **unoffical** project. Use at your own risk and with responsibility.

NTU COOL (cool.ntu.edu.tw) CLI + MCP server.

Also comes with NTU COOL SDK. Currently supported languages:
- Rust
In the future, followings may be supported:  
- Python
- JavaScript / TypeScript

NTU-only: login is NTU's ADFS SAML flow. Interfaces inherit from Canvas LMS but are adapted to NTU Canvas instance.

## Install

### Claude Code

> TODO: claude code plugin installation

### CLI

> TODO: make these collapsable

Prebuilt binaries (Linux / Windows — no Rust needed):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.sh | sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.ps1 | iex"
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.ps1 | iex"
```

macOS, or any platform from source:

```sh
cargo install ntucool ntucool-mcp
```

(macOS prebuilt binaries are pending — GitHub's free-tier macOS runner queue is currently sitting on jobs without picking them up. Tracking for a future release.)

Then:

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


TUI (WIP)
<details>
There are also TUI supported for some commands, e.g. `cool assignment`. 
</details>

`cool --help` for the full command surface.

### Summary of supported CLI commands

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

Check `docs/TOOLS.md` and `plugins/ntucool/skills/ntucool/SKILL.md` for more comprehensive (yet might be more unreadable and noisy) descriptions.

## Security issues

1. Credentials

2. Write operations

3. Abuses
Please don't abuse the tools this project provides.

## NTU COOL SDK

Under development. Check `codegen/` for current pipeline and `cool-api/` for the Rust client generated.

## How it works

- NTU COOL is based on Canvas LMS, an open source platform.
- We collect Canvas LMS API schemas from offical resources.
- Transform into per-language SDK via `codegen/`.
- Both CLI and MCP use generated Rust client for API requests.

### Auth procedure

NTU COOL does not support original Canvas auth token. 
Therefore, we have to manually emulate the NTU SAML login and extract the cookies. Doing so currently requires typing student account and password in terminal with `cool login`.
The cookies is saved in `TODO`, and is attached in every API requests.
The cookies automatically expire after 24 hrs on NTU COOL's side.


## License

MIT.
