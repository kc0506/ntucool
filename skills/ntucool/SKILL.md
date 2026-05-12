---
name: ntucool
description: NTU COOL (cool.ntu.edu.tw) — National Taiwan University's Canvas LMS. Use when working with NTU course materials, assignments, grades, announcements, or PDFs. Provides CLI (`cool`) and MCP server (`ntucool-mcp`). NTU-only (ADFS SAML auth).
---

# ntucool

NTU COOL is National Taiwan University's Canvas LMS instance at `https://cool.ntu.edu.tw`. This skill covers the `cool` CLI and `ntucool-mcp` MCP server distributed at <https://github.com/kc0506/ntucool>.

**Scope**: NTU students working with their own enrolments — list/search courses, find PDFs, read assignments, check grades and submissions. Not for admins. Not for non-NTU Canvas instances (login is hardcoded to NTU's ADFS SAML SSO).

## Quickstart (one-time setup)

Skip this section if `cool` and `ntucool-mcp` are already on PATH and a session exists.

Install the binaries (prebuilt — no Rust toolchain needed). The CLI and the MCP server are separate crates, so each ships its own installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.sh | sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-installer.ps1 | iex"
powershell -ExecutionPolicy ByPass -c "irm https://github.com/kc0506/ntucool/releases/latest/download/ntucool-mcp-installer.ps1 | iex"
```

Or from source if Rust is available: `cargo install ntucool ntucool-mcp`.

Then log in once:

```sh
cool login                            # writes session.json under $XDG_DATA_HOME/ntucool/
```

`cool login` prompts NTU credentials and asks how to save them for non-interactive re-login (plaintext-in-credentials.json / password-cmd / never-save).

The MCP server reads the session/credentials written by `cool login`; there is **no MCP login flow**. If the session expires (cookies last ~24h), both CLI and MCP transparently re-login from saved credentials. If re-login fails, run `cool login` again.

### First-run failure modes

The plugin manifest cannot install binaries — if setup is incomplete, failure surfaces as a tool-call error rather than a setup wizard. Map errors back to the fix:

- **MCP server fails to start** / `ntucool-mcp: command not found` → binary not on PATH. Show the install one-liner above and stop; the agent cannot proceed without it.
- **Tool returns `not_logged_in` / 401 / "session expired and re-login failed"** → tell the user to run `cool login` in a terminal. MCP cannot prompt for credentials.
- **`cool login` succeeded but MCP still 401s** → confirm Claude Code is running as the same user that ran `cool login` (`$HOME` mismatch is the usual cause when running inside a sandbox / different shell session).

## When to use CLI vs MCP

Same tool surface, two access paths. Both read the same session/credentials.

- **MCP** (this plugin wires it up via `.mcp.json`): use when an AI agent is the caller — "find me X", "what's due", reading content into the conversation.
- **CLI**: suggest a shell command when the action is auth, bulk, or destructive:
  - Auth lifecycle: `cool login`, `cool logout --purge` — CLI-only by design.
  - Bulk download: `cool module download <course_id> <module_id>` streams files to disk with multi-file progress bars.
  - Adhoc inspection from a human's terminal.

## MCP usage (declarative)

### Tools at a glance

| Domain | Tools |
|---|---|
| Identity | `whoami`, `users_get` |
| Courses | `courses_list`, `courses_resolve`, `courses_get` |
| Files | `files_list`, `files_search`, `files_get_metadata`, `files_fetch` |
| Assignments | `assignments_list`, `assignments_get` |
| Announcements | `announcements_list`, `announcements_get` |
| Modules | `modules_list`, `modules_get` |
| Discussions | `discussions_list`, `discussions_get` |
| Pages | `pages_list`, `pages_get` |
| PDFs | `pdf_search`, `pdf_extract` |
| Self grading | `submissions_mine`, `grades_get` |

23 tools. Parameter shapes live in `docs/TOOLS.md`. The subsections below cover *why and when* — judgement that doesn't fit in a JSON schema.

### Resolving courses

Don't guess `course_id`. Two flows:

- Fuzzy name → `courses_resolve("substring")` returns ranked matches with scores. Numeric IDs always resolve (no API hit, score 1.0).
- Browse → `courses_list` (active by default; `filter="all"` for past enrolments).

### Self contact info: `whoami` not `users_get`

`whoami` returns the rich self profile with `login_id` and `primary_email`. `users_get(user_id)` returns much less — even when you pass your own id, Canvas hides `login_id` and `email` for student-level sessions at the `/users/:id` endpoint, not just for other users. So use `whoami` for your own contact info; reserve `users_get` for resolving teachers / classmates by id (name + avatar only).

### Finding course content

- By filename: `files_search`. `course_id` optional — omit to search across every accessible course. Canvas requires queries ≥ 3 bytes.
- By PDF content: `pdf_search(course_id, query)`. **First call per course is slow** — downloads and extracts every PDF in the course (minutes for a 50-PDF course). Subsequent calls reuse the on-disk text cache and complete in seconds.
- To read more context around a hit: `pdf_extract(file_id, pages="<page>-<page+2>")`.

**Single-course limitation**: `pdf_search` does not span courses. If you don't know which course owns the content, you must iterate active courses yourself — each one will warm its own cache the first time.

### Reading assignments / announcements / pages

`*_get` returns `description_md` / `body_md` (HTML→Markdown via htmd). Pass `with_html=true` only when the markdown rendering loses something you care about (rare).

Each detail object carries `references[]` — a typed union of Canvas-internal links mined from the body HTML. Dispatch on `kind`:

- `File` → `files_fetch` / `files_get_metadata` with the embedded `id`
- `Page` → `pages_get(course_id, slug)`
- `Assignment` / `DiscussionTopic` / `Module` → the matching `*_get`

Don't regex the URLs. They're there as a fallback for humans.

### Submissions and grades

`submissions_mine(course_id?, status?)` returns one entry per assignment. **Includes unsubmitted entries by default** — useful for "what's still due". Filter `status="graded"` for "what have I gotten back". `score` is numeric points; `grade` is the rendered grade (letter / pass-fail / numeric-as-string depending on assignment grading_type).

`grades_get(course_id?)` returns per-course grade summaries with two pairs of fields:

- `current_*` reflects only graded work to date — the right field mid-semester.
- `final_*` treats every ungraded assignment as **zero**. NTU therefore shows most active courses as `final_score: 0.0`, `final_grade: "X"`. **Ignore `final_*` until the semester is actually over**, or you'll mislead the user that they're failing.

### Files: metadata vs bytes

- `files_get_metadata(file_id)` returns Canvas's internal `url` field — this needs a logged-in browser, not usable from an MCP context.
- `files_fetch(file_id)` returns a `file://` URI (in stdio mode) pointing at a server-internal cache file the MCP server has already downloaded for you. Use this when bytes are actually needed (downstream `pdf_extract`, viewer, etc.). Cached by Canvas's `updated_at`, so repeat calls are free until the file changes.

## CLI commands

`cool --help` for the full tree. The CLI is *not* a mirror of MCP — auth lives only here, and bulk/upload flows are CLI-only by design. Conversely, several read-only agent tools have no CLI surface yet.

| Command | Purpose |
|---|---|
| `cool login` | Interactive ADFS login + save credentials. **CLI-only**. |
| `cool logout [--purge]` | Drop session cookies; `--purge` also deletes credentials.json. **CLI-only**. |
| `cool whoami` | Self profile (includes `login_id` + `primary_email`). |
| `cool course list [--all] [--term <id>]` | List enrolments. |
| `cool course show <course_id>` | Course detail (syllabus, term, teachers). |
| `cool assignment ...` | Assignment list/show (see `cool assignment --help`). |
| `cool announcement list/show` | Announcements per course. |
| `cool discussion list/show` | Discussion topics. |
| `cool module list/show` | Modules; `list` returns items inline. |
| `cool module download <course> <module>` | Bulk file download with multi-progress bars. **CLI-only**. |
| `cool file ls/download/upload` | File ops. **`upload` is CLI-only**. |
| `cool user get <user_id>` | Look up another user (name + avatar; no email at student privilege). |
| `cool submission mine [--course] [--status]` | Self submissions across one / all active courses. |
| `cool grade [--course <id>]` | Per-course grade summary. |

Append `--json` to any list/show command for raw JSON.

**MCP-only (no CLI yet)**: `courses_resolve`, `files_search`, `files_get_metadata`, `pages_list`, `pages_get`, `pdf_search`, `pdf_extract`. These are agent-oriented read operations — from a human terminal you can hit the MCP server directly or file an issue if you'd like a CLI counterpart.

Caveats shared with MCP (`final_*` zero projection, `pdf_search` cold-cache cost, `users_get` privacy filtering) apply identically wherever a command's surface overlaps.

## Reference

- Repository: <https://github.com/kc0506/ntucool>
- Full MCP tool spec: [`docs/TOOLS.md`](https://github.com/kc0506/ntucool/blob/main/docs/TOOLS.md)
