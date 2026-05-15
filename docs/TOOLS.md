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

### Tier 2 — Status

| Tool | 狀態 | 備註 |
|---|---|---|
| `todo_upcoming(window=7d)` | ❌ | `/users/self/upcoming_events` + `/todo` |
| `submissions_mine(course_id?, status?)` | ✅ | `/api/v1/courses/:cid/students/submissions?student_ids[]=self&include[]=assignment` → `[SubmissionMine {course_id, assignment_id, assignment_name?, points_possible?, score?, grade?, workflow_state?, submitted_at?, graded_at?, late?, missing?, excused?}]`。`course_id` 省略時掃所有 active 課 (per-course N+1, sequential)。`status` 可選 `submitted`/`unsubmitted`/`graded`/`pending_review`。**Codegen workaround**: 該 endpoint 在 codegen 中 type 成 `Result<()>` (no response schema) — bypass 用自定義 `RawSubmission`。 |
| `grades_get(course_id?)` | ✅ | `/api/v1/users/self/enrollments?type[]=StudentEnrollment&include[]=current_grade` → `[CourseGrade {course_id, course_name?, current_grade?, current_score?, final_grade?, final_score?, html_url?}]`。`current_*` 只算 graded 部分; `final_*` 把 ungraded 視為零 (NTU 大多顯示 X / 0.0)。**Codegen workaround #2**: codegen `Grade.current_score`/`final_score` type 成 `Option<String>` 但 NTU 實際回 float — 同 #21 bug 家族; bypass 用自定義 `RawEnrollment` + `Option<f64>`。 |
| `calendar_events(courses?, range)` | ❌ | |
| `activity_recent(limit=20)` | ❌ | `/users/self/activity_stream` |
| `users_get(user_id)` | ✅ | `/api/v1/users/:id` → `UserSummary {id, name, short_name, sortable_name, login_id?, email?, avatar_url?}`. **NTU 實測**: student 等級看任何人 (含 self) 都拿不到 `login_id` 和 `email` — Canvas 在 endpoint 層做隱私過濾, 不只是 user 層。要拿 self 完整資訊請用 `whoami`。 |

### Tier 3 — Write

| Tool | 狀態 | 風險 | 備註 |
|---|---|---|---|
| `assignments_submit(course_id, assignment_id, files\|text, comment?)` | ✅ CLI + MCP | **高** | 見下方「Submit 風險閘」 |
| `files_upload(course, path, dest?)` | ❌ | 中 | |
| `discussions_reply(topic_id, body)` | ❌ | **高** | 預設拒絕 |
| `announcements_mark_read(id)` | ❌ | 低 | |

#### 權限模型:`write_level`

寫入由單一 ordinal `write_level` 控制,CLI 與 MCP **共用同一份設定**:

| `write_level` | 乾淨提交 | Soft 風險 | Hard 風險 |
|---|---|---|---|
| `none`(預設) | ✗ 全擋 | ✗ | ✗ |
| `safe` | ✓ | ✗(`i_understand` 無效) | ✗ |
| `guarded` | ✓ | 需 `i_understand` | ✗ |
| `unguarded` | ✓ 跳過 preflight | ✓ | ✓(直接送,Canvas 自己拒) |

設定來源(優先序高→低):

1. 環境變數 `NTUCOOL_WRITE_LEVEL`(值 `none`/`safe`/`guarded`/`unguarded`,或 `0`–`3`)— 給一次性 CLI 用,不必先建檔
2. `.ntucool.json` — 從 cwd 往上找最近一份,`{ "write_level": "guarded" }`;固有 project / MCP 啟動通用
3. 內建預設 `none`

解析在 `cool_api::config`。注意這是**便利 / 安全網,不是 security boundary** — 能改檔案或設環境變數的 agent 一樣能改它;要真正擋住請用 Claude Code 自身的工具權限層。

#### Submit 風險閘

`assignments_submit` 不直接送出 — `none`/`unguarded` 以外都先跑 `preflight` 驗風險:

- **Hard 風險(`safe`/`guarded` 永遠 abort)**:`type_mismatch`(assignment 不收這個 submission type)、`locked`(`locked_for_user`)、`not_yet_unlocked`(`unlock_at` 未到)、`past_lock_date`(`lock_at` 已過)、`disallowed_extension`(副檔名不在 `allowed_extensions`)、`attempts_exhausted`(用完 `allowed_attempts`)。Canvas 一定會拒的提交。
- **Soft 風險**:`past_due`(過 `due_at`,會記 late)、`overwrites_existing`(已有提交,會新增一次 attempt)。`safe` 一律擋、`guarded` 帶 `i_understand` 放行。

前端如何取得同意:

- **CLI** (`cool assignment submit`):印出 preflight 摘要 + `write_level` + 風險清單;`none` 在 preflight 前就早擋;`safe` 遇任何風險直接 abort;`guarded` 跳互動確認(預設 No,非 TTY 視為 No),`--i-understand` 跳過確認;`unguarded` 不確認直接送。
- **MCP** (`assignments_submit`):`confirm=false`(預設)只回 `SubmitPreflight` 預覽、永遠可跑(連 `none` 也能預覽);`confirm=true` 才送,Soft 風險需另帶 `i_understand=true`。AI 不得在使用者未明示下自行帶 `confirm`/`i_understand`。被擋時錯誤訊息會說明怎麼調 `write_level`。

成功回 `SubmissionReceipt {course_id, assignment_id, workflow_state?, submission_type?, submitted_at?, attempt?, late?, preview_url?}`。

**Codegen workaround #3**:codegen 的 `SubmitAssignmentCoursesParams` 把 `submission[submission_type]` 之類欄位 serde-rename 成點號扁平 key(`"submission.submission_type"`),Rails 不會還原成巢狀 → `endpoints::submit_assignment_courses` 不可用。`cool-tools::assignments::submit` bypass codegen,手建巢狀 JSON `{"submission":{…},"comment":{…}}` 並用 `client.post` 取回 `Submission`。

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
- Tier 3 寫工具:`write_level` 權限模型(`none`/`safe`/`guarded`/`unguarded`)+ preflight 風險閘,見「Tier 3 — Write」

## 非目標(明確不做)

- Quiz tools(學術誠信)
- 直接代答作業(只給資料 / draft,不自動 submit 文字内容)
- OCR 掃描 PDF(目前依賴 `pdf-extract`;掃描檔回空 pages,`empty=true` 標出來)
- Web UI / 多用戶帳號管理
