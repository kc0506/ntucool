# Tool Surface — NTU COOL AI Assistant

對齊文件:工具表面、phase 規劃、設計原則。前端 (CLI / MCP) 怎麼長都不影響這份。

`cool-tools/src/types.rs` 是 contract 的真實。這份文件描述每個 tool **應該是什麼**,當實作與描述不一致時,以 `types.rs` + tool description 為準,並更新本文。

## 設計原則

1. **以「AI 一句話請求」回推**,不是以 Canvas API 劃分回推
2. **JSON-first / ID-stable**:每個 tool 結構化輸出,內部 ID 是工具串聯的契約
3. **Read 優先,Write 嚴格**:讀工具無腦給;寫工具預設關、需明確 opt-in,且永遠不暗中執行
4. **Resolver 是一級工具**(不是內部 helper):AI 講「解剖學」要能拿到 course_id,**多 match 時回多筆**讓 AI 挑
5. **避免讓 AI 做路徑遍歷**:給 `files_search`、`modules_list(with_items=true)`,不要逼它一層層 ls
6. **可組合**:工具 A 的輸出欄位,工具 B 直接吃。`CanvasRef` tagged union 是這層的具體展現
7. **不洩漏 server 內部路徑**:`files_fetch` 的 URI 只反映 publish 模式,不會暴露 server 自己的 cache 位置
8. **預設 markdown,raw HTML 是 opt-in**:省 token,但不剝奪 AI 看原始描述的能力

## 後端 / 前端分層

```
cool-api/      Canvas API client (generated endpoints + auth)
cool-tools/    純邏輯,純 struct in / struct out,無 IO formatting    ← 唯一真實
cool-cli/      args parsing + table/json formatting                  ← 薄殼
cool-mcp/      MCP server,tool schema + JSON adapter                 ← 薄殼
```

`cool-tools` 是契約。CLI 跟 MCP 都呼叫它,輸入輸出都是 plain Rust struct,類型派生 `Serialize/Deserialize/JsonSchema` 給 MCP `Parameters<>` 用。
任何 `println!` / table render / progress bar 都不該出現在 `cool-tools`。

## 命名

- 動詞用 RPC convention:`courses_get`、`assignments_get`、`pages_get`(原 `*_show` 已全部改名)
- 帶 id 的 args qualified 命名:`assignment_id`、`topic_id`、`module_id`,避免在多 arg context 下歧義
- File 是全域 ID(`/api/v1/files/:id` 可用),所以 `files_get_metadata`、`files_fetch` 不需要 `course_id`
- Assignment / Discussion / Module / Page 全部 course-scoped,Canvas `/:id` 端點 404,所以一律帶 `course_id`

## Probe 結論(已驗證)

| 端點 | 結論 |
|---|---|
| `GET /courses/:id/smartsearch` | **401 across all courses on NTU COOL** — 不可用,不寫 wrapper |
| `list_files_courses?search_term=...` | OK,server-side 子字串。3 byte 最低長度 |
| `list_assignments?search_term=...` | OK |
| `list_modules?search_term=...` | OK,只比 module name |
| `list_announcements` | 必須帶 `context_codes[]` (HTTP 400 otherwise);empty list = client-side fallback to active enrolments |
| 中文 1 字元 search_term | **400 Bad Request** — UI 須拒絕 < 3 byte query |
| `/api/v1/{assignments,discussion_topics,modules}/:id` | **404** — 必須帶 `course_id` (driven this layer's design) |
| `/api/v1/files/:id` | OK 全域,no course scope needed |
| Canvas float-vs-int | `RubricCriteria.points` / `RubricRating.points` / `ScoreStatistic.*` 都回 float (e.g. 5.0),codegen 標 i64 ⇒ 已手動 patch 成 f64 |
| pdf-extract `extract_text` | 不插 form-feed,單一 string;**改用 `extract_text_by_pages`** 拿 `Vec<String>` |

## 工具列表

### Tier 0 — Resolver / 入口

| Tool | 實作 | 備註 |
|---|---|---|
| `whoami` | ✅ | `ProfileSummary {id, name, login_id, primary_email}` |
| `courses_list(filter=active|all, term?)` | ✅ | `[CourseSummary {id, name, course_code, term}]` |
| `courses_resolve(query)` | ✅ | active 沒 hit 才 fallback 到 all-enrolments(過往學期亦可解析);numeric ID score=1.0 |
| `courses_get(course_id)` | ✅ | `CourseDetail` 含 syllabus / term / start_at / end_at / time_zone / teachers |

### Tier 1 — Files

| Tool | 實作 | 備註 |
|---|---|---|
| `files_list(course_id, path?)` | ✅ | folder tree;path 預設根目錄 `/` |
| `files_search(course_id?, query)` | ✅ | course_id None ⇒ 跨課搜 `/users/self/files`;query >= 3 bytes |
| `files_get_metadata(file_id)` | ✅ | 全域 ID;`url` 是 Canvas signed URL,需 session cookie 才能存取 — bytes 用 `files_fetch` |
| `files_fetch(file_id)` | ✅ | 透過 cool-mcp cache。stdio ⇒ `file://<output_dir>/...`;http ⇒ `http://host/files/<token>`(短期簽名 URL,TTL 預設 3600s)。Server-internal cache (`$XDG_CACHE_HOME/cool-mcp/cache/`) 永遠不出現在 URI 裡 |

### Tier 1 — Activity / Content

| Tool | 實作 | 備註 |
|---|---|---|
| `assignments_list(course_id, bucket?)` | ✅ | bucket: `upcoming` (≤7d, unsubmitted) / `future` (>7d) / `overdue` / `past` / `undated` / `ungraded` / `unsubmitted`。**注意 7 天分界 — 大於 1 週要省略 bucket 或同時查兩個** |
| `assignments_get(course_id, assignment_id, with_html?=false)` | ✅ | `description_md` (htmd) + `references` (CanvasRef tagged union) + rubric;`with_html=true` 才會塞 raw HTML |
| `announcements_list(course_ids?, since?)` | ✅ | empty list ⇒ active 全課;`since` 是 ISO-8601 |
| `announcements_get(course_id, topic_id, with_html?)` | ✅ | `body_md` + author_name + references |
| `modules_list(course_id, with_items?=false)` | ✅ | `with_items=true` ⇒ 一發拿到 `[ModuleDetail]`(含 items),省去逐 module 呼叫 |
| `modules_get(course_id, module_id)` | ✅ | items 含 type / content_id / url |
| `discussions_list(course_id)` | ✅ | summary 含 author |
| `discussions_get(course_id, topic_id, with_entries?, with_html?)` | ✅ | with_entries 預設 true(拉第一層 entries) |
| `pages_list(course_id)` | ✅ | URL slug 是 page 主鍵 |
| `pages_get(course_id, url, with_html?)` | ✅ | `body_md` (htmd) + references |

### Tier 2 — PDF Content

| Tool | 實作 | 備註 |
|---|---|---|
| `pdf_extract(file_id, pages?)` | ✅ | `pages` 接受 "all"(預設) / "5" / "5-10";per-page text 經 `pdf_extract::extract_text_by_pages`,結果 cache 在 `$XDG_CACHE_HOME/cool-mcp/text/<id>-<ts>.json` 與 bytes cache 一同失效 |
| `pdf_search(course_id, query, max_results?=20)` | ✅ | 列課程 PDF (`content_types[]=application/pdf`),逐檔抽文(首呼叫慢、後續秒回);unparseable PDF 跳過不致命;回 `[PdfSearchHit {file_id, display_name, page, snippet}]` |

### Tier 2 — Status (尚未做)

| Tool | 狀態 | 備註 |
|---|---|---|
| `todo_upcoming(window=7d)` | ❌ | `/users/self/upcoming_events` + `/todo` |
| `submissions_mine(course?, status?)` | ❌ | 我交了沒/分數/缺什麼 |
| `calendar_events(courses?, range)` | ❌ | |
| `activity_recent(limit=20)` | ❌ | `/users/self/activity_stream` |
| `users_get(user_id)` | ❌ | resolve user_id → name/avatar |

### Tier 3 — Write (尚未對 MCP 暴露)

| Tool | 狀態 | 風險 | 備註 |
|---|---|---|---|
| `files_upload(course, path, dest?)` | CLI only | 中 | |
| `assignments_submit(id, files\|text, comment?)` | CLI only MUT | **高** | dry-run + 二次確認 |
| `discussions_reply(topic_id, body)` | ❌ | **高** | 預設拒絕 |
| `announcements_mark_read(id)` | ❌ | 低 | |

MCP server 啟動時須白名單啟用 write tools(尚未實作);CLI 模式由人類 gatekeep。

### Tier 4 — Local 維運(🔒 只 CLI)

| Tool | 狀態 | 備註 |
|---|---|---|
| `login` 🔒 | ✅ | 互動式;saved credentials 走 `password_cmd`(blank ⇒ 不存) |
| `logout [--purge]` 🔒 | ✅ | 刪 session.json;`--purge` 同時刪 credentials.json |
| `cache_clear(scope=files\|text\|all)` 🔒 | ❌ | |

## CanvasRef tagged union

`assignments_get` / `announcements_get` / `discussions_get` / `pages_get` 都回 `references: Vec<CanvasRef>`,從 body HTML 抽出 Canvas 內部連結:

```jsonc
{ "kind": "File",            "id": 9652398,                "name": "hw3.pdf", "href": "..." }
{ "kind": "Page",            "course_id": 61640, "slug": "syllabus", ... }
{ "kind": "Assignment",      "course_id": 61640, "id": 378287, ... }
{ "kind": "DiscussionTopic", "course_id": 61640, "id": 495057, ... }
{ "kind": "Module",          "course_id": 61640, "id": 861762, ... }
```

每個 variant 帶夠資訊直接 dispatch 到對應的 `*_get` / `files_fetch`。

## 持久化 / 狀態

```
$XDG_DATA_HOME/ntucool/session.json           登入 cookies (cool-cli 寫,cool-api 讀)
$XDG_CONFIG_HOME/ntucool/credentials.json     username + password_cmd (optional)
$XDG_CACHE_HOME/cool-mcp/cache/<id>-<ts>.<ext>  files_fetch 抓回的原檔 (server-internal,不對外)
$XDG_CACHE_HOME/cool-mcp/text/<id>-<ts>.json    PDF 抽文 sidecar (server-internal)
$XDG_DATA_HOME/cool-mcp/files/<id>/<name>     stdio publisher 給 client 的 copy (file://...)
```

所有 XDG path resolution 走 `cool_api::paths` 模組,collapse 從前每個 caller 各自手刻 `env::var("XDG_*")` + `HOME` fallback 的 7 處重複。

Session TTL 經驗值 ~24 小時(NTU 走 ADFS SAML,cookie 沒有 Max-Age)。`Session::is_likely_expired()` 過了 24h 就 warn,MCP startup log 會帶 `age_hours`。

**Auto re-login**: `CoolClient` 上每個 HTTP 方法在 401 時走 `try_relogin_if_stale` chain — 讀 `Credentials`(走 sentinel-aware `resolve_password`,擋 placeholder `echo TODO` 之類)→ `saml_login` → 寫回 session.json → retry 一次。並發 401 經由 `relogin_lock` + generation counter 做 single-flight,不會 fan-out N 次 saml_login。`CoolClient::from_default_session_lazy()` 允許在沒 session.json 時起服務,首次 request 由同一條 chain 觸發初次登入。

## File Serving (cool-mcp)

兩個獨立概念,不可混用:

1. **Server-internal cache** — `$XDG_CACHE_HOME/cool-mcp/cache/`,Canvas bytes 一份,key 是 `(file_id, updated_at_unix)`。Client 永遠看不到此路徑。
2. **Public publisher** — 把 cached file 變成 client 可讀的 URI:
   - **stdio mode** (`COOL_MCP_FILE_MODE=stdio`,預設): copy 到 `$XDG_DATA_HOME/cool-mcp/files/<id>/<name>` 並回 `file://...`。Output dir 可用 `COOL_MCP_OUTPUT_DIR` 覆寫。
   - **http mode** (`COOL_MCP_FILE_MODE=http`): 啟動 axum 在 `COOL_MCP_HTTP_BIND` (預設 `127.0.0.1:0`),mint 32-hex-char token,回 `http://host:port/files/<token>`,TTL 由 `COOL_MCP_HTTP_TTL_SECS` 控制(預設 3600)。同 cache 在 TTL 內重複呼叫拿同一 token。`COOL_MCP_HTTP_PUBLIC_BASE` 用於反向代理。

## Rate limit / 風險

- Concurrency cap:read 4、download 2(已有 module download 在用)
- 429 處理:讀 `Retry-After`,沒給就 1→2→4→8s exponential + jitter,上限 30s
- 中文 query length guard:< 3 byte 不送請求(server 拒絕)
- Tier 3 寫工具 dry-run + confirmation 流程定義稍後

## 非目標(明確不做)

- Quiz tools(學術誠信)
- 直接代答作業(只給資料 / draft,不自動 submit 文字内容)
- OCR 掃描 PDF(目前依賴 `pdf-extract`;掃描檔回空 pages,`empty=true` 標出來)
- Web UI / 多用戶帳號管理
