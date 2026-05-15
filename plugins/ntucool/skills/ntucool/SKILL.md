---
name: ntucool
description: NTU COOL (cool.ntu.edu.tw) — National Taiwan University's Canvas LMS. Use when working with NTU course materials, assignments, grades, announcements, or PDFs. Provides CLI (`cool`) and MCP server (`ntucool-mcp`). NTU-only (ADFS SAML auth).
---

# ntucool

NTU COOL is National Taiwan University's Canvas LMS instance at `https://cool.ntu.edu.tw`. This skill covers the `cool` CLI and `ntucool-mcp` MCP server distributed at <https://github.com/kc0506/ntucool>.

**Scope**: NTU students working with their own enrolments — list/search courses, find PDFs, read assignments, check grades and submissions. Not for admins. Not for non-NTU Canvas instances (login is hardcoded to NTU's ADFS SAML SSO).

## Setup gate — run BEFORE the first ntucool tool call

The plugin manifest can't install binaries or prompt for a password, so the first interaction in any session must verify setup. Do this *once* per conversation, before any ntucool tool/CLI call:

```bash
which cool ntucool-mcp 2>/dev/null && cool whoami 2>&1 | head -1
```

- All three lines present and `whoami` returns user info → setup OK, proceed.
- Anything missing or `whoami` errors with 401 / "not logged in" / "command not found" → setup is incomplete. Drive the user through it before answering their original question.

The full setup procedure (install via prebuilt installer + `cool login`) is in the `/ntucool:setup` slash command. Either:

- Tell the user to run `/ntucool:setup` and wait, OR
- Run the install step yourself (it's a single `curl … | sh` per binary, see `commands/setup.md`), then have the user type `! cool login` (interactive — Bash tool can't drive the password prompt).

After setup completes, resume the user's original task without making them re-ask.

### Why the gate matters

The MCP server reads `session.json` + `credentials.json` written by `cool login`; there is **no MCP login flow**. If the session expires (cookies last ~24h), both CLI and MCP transparently re-login from saved credentials. If saved credentials are absent or stale, calls 401 deep inside the API client and surface as opaque tool errors. The gate catches that before you make the user wait through a failed tool call.

`$HOME` / sandbox mismatch is the rare third failure mode: `cool login` writes to the user's `$HOME`, but if Claude Code is running under a different user (sandbox, container), MCP won't see the session file. Confirm `cool whoami` works in the same shell environment the MCP server runs in.

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
| Assignments | `assignments_list`, `assignments_get`, `assignments_submit` |
| Announcements | `announcements_list`, `announcements_get` |
| Modules | `modules_list`, `modules_get` |
| Discussions | `discussions_list`, `discussions_get` |
| Pages | `pages_list`, `pages_get` |
| PDFs | `pdf_search`, `pdf_extract` |
| Self grading | `submissions_mine`, `grades_get` |

24 tools — 23 read-only plus `assignments_submit` (the only write tool). Parameter shapes live in `docs/TOOLS.md`. The subsections below cover *why and when* — judgement that doesn't fit in a JSON schema.

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

### Submitting an assignment — the only write tool

`assignments_submit` turns work in. It is the one tool that changes the user's record, so it is gated harder than anything else:

- **Gated by `write_level`.** A single ordinal — `none` (default, refuses every submit) / `safe` (only risk-free submissions) / `guarded` (`i_understand` clears soft risks) / `unguarded` (no checks). Resolved from env `NTUCOOL_WRITE_LEVEL`, else the nearest `.ntucool.json` walking up from cwd, else `none`. When a submit is refused the error states how to raise the level — relay that to the user; don't change `write_level` yourself.
- **Two-stage.** Call with `confirm=false` first (the default) — it returns a `SubmitPreflight` (what would be submitted + every risk) and submits *nothing*. `confirm=false` previews work at any level, including `none`. Show it to the user. Only call `confirm=true` after they explicitly say to submit.
- **Never auto-confirm.** Do not set `confirm` or `i_understand` on your own initiative — both require the user's explicit go-ahead in the conversation.

Risks come back with severity `hard` or `soft`. Hard (wrong submission type, locked, past lock date, disallowed file extension) abort at `safe`/`guarded` — Canvas would reject them anyway. Soft (`past_due` → late, `overwrites_existing` → new attempt) abort unless `write_level` is `guarded` and you pass `i_understand=true`.

Provide either `files` (paths on the machine running cool-mcp → `online_upload`) or `text` (→ `online_text_entry`), never both. Per the non-goals: don't generate the answer content yourself — submit only what the user gives you.

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

Append `--json` to any list/show command for raw JSON.

**MCP-only (no CLI yet)**: `courses_resolve`, `files_search`, `files_get_metadata`, `pages_list`, `pages_get`, `pdf_search`, `pdf_extract`. These are agent-oriented read operations — from a human terminal you can hit the MCP server directly or file an issue if you'd like a CLI counterpart.

Caveats shared with MCP (`final_*` zero projection, `pdf_search` cold-cache cost, `users_get` privacy filtering) apply identically wherever a command's surface overlaps.

## Reference

- Repository: <https://github.com/kc0506/ntucool>
- Full MCP tool spec: [`docs/TOOLS.md`](https://github.com/kc0506/ntucool/blob/main/docs/TOOLS.md)
