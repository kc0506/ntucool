# Tool Surface — NTU COOL AI Assistant

對齊文件:工具表面、phase 規劃、設計原則。前端 (CLI / MCP) 怎麼長都不影響這份。

## 設計原則

1. **以「AI 一句話請求」回推**,不是以 Canvas API 劃分回推
2. **JSON-first / ID-stable**:每個 tool 結構化輸出,內部 ID 是工具串聯的契約
3. **Read 優先,Write 嚴格**:讀工具無腦給;寫工具預設關、需明確 opt-in,且永遠不暗中執行
4. **Resolver 是一級工具**(不是內部 helper):AI 講「解剖學」要能拿到 course_id,**多 match 時回多筆**讓 AI 挑
5. **避免讓 AI 做路徑遍歷**:給 `files.search`、`files.get_by_path`,不要逼它一層層 ls
6. **可組合**:工具 A 的輸出欄位,工具 B 直接吃

## 後端 / 前端分層

```
cool-api/      Canvas API client (generated endpoints + auth)
cool-tools/    純邏輯,純 struct in / struct out,無 IO formatting    ← 唯一真實
cool-cli/      args parsing + table/json formatting                  ← 薄殼
cool-mcp/      MCP server,tool schema + JSON adapter                 ← 薄殼
```

`cool-tools` 是契約。CLI 跟 MCP 都呼叫它,輸入輸出都是 plain Rust struct(後續可加 `Serialize` 給 MCP)。
任何 `println!` / table render / progress bar 都不該出現在 `cool-tools`。

## Probe 結論(已驗證)

| 端點 | 結論 |
|---|---|
| `GET /courses/:id/smartsearch` | **401 across all courses on NTU COOL** — 不可用,不寫 wrapper |
| `list_files_courses?search_term=...` | OK,server-side 子字串 |
| `list_assignments?search_term=...` | OK |
| `list_modules?search_term=...` | OK,只比 module name |
| `list_announcements` | **無 `search_term`** — client-side filter title |
| 中文 1 字元 search_term | **400 Bad Request** — UI 須拒絕 < 2 chars query |

## 工具列表

標記:`[CLI]` 已 wire / `[GEN]` generated 有 / `[NEW]` 新邏輯 / `MUT` 寫操作 / 🔒 不對 MCP 暴露

### Tier 0 — Resolver / 入口

| Tool | 狀態 | 備註 |
|---|---|---|
| `whoami` | [CLI] | sanity check |
| `courses.list(filter?, term?)` | [CLI] | |
| `courses.resolve(query)` | [NEW] | 名稱/代碼/縮寫 → `[{id, name, score}]`,**多 match 全回** |
| `courses.show(id)` | [GEN] | syllabus / term / teachers |

### Tier 1 — Read tools(主力)

| Tool | 狀態 | 備註 |
|---|---|---|
| `files.list(course, path=?)` | [CLI] | 樹狀概觀 |
| `files.search(q, course?)` | [NEW Phase 1] | `search_term`;query length guard |
| `files.get_metadata(file_id)` | [GEN] | size/mime/url/updated_at |
| `files.download(file_id, dest?)` | [CLI] | |
| `files.get_text(file_id, pages=?)` | [NEW Phase 2] | **PDF→text + 分頁。AI 讀資料的關鍵入口** |
| `assignments.list(course, bucket=upcoming/past/overdue/...)` | [CLI] | bucket 是 Canvas 原生 |
| `assignments.show(id)` | [CLI] | full description, rubric, attachments |
| `announcements.list(courses=[], since=?)` | [CLI] | 跨課,client-side title filter |
| `announcements.show(id)` | [NEW thin] | body |
| `modules.list(course)` | [CLI] | |
| `modules.show(id)` | [CLI] | items 含 type/url/content_id |
| `discussions.list(course)` | [CLI] | |
| `discussions.show(id, with_entries=true)` | [GEN] | 含留言串 |
| `pages.list(course)` + `pages.show(course, url)` | [GEN] | Canvas wiki page |
| `content.search(q, course?)` | [NEW Phase 2] | tantivy 全文搜跨 PDF |

### Tier 2 — 狀態(讓 AI 主動回報用)

| Tool | 狀態 | 備註 |
|---|---|---|
| `todo.upcoming(window=7d)` | [GEN] | `/users/self/upcoming_events` + `/todo` |
| `submissions.mine(course?, status=missing/graded/submitted)` | [GEN] | 我交了沒/分數/缺什麼 |
| `calendar.events(courses=[], range)` | [GEN] | |
| `activity.recent(limit=20)` | [GEN] | `/users/self/activity_stream` |

### Tier 3 — Write(MUT,白名單啟用)

| Tool | 狀態 | 風險 | 備註 |
|---|---|---|---|
| `files.upload(course, path, dest?)` | [CLI] | 中 | |
| `assignments.submit(id, files=[]\|text=..., comment?)` | [GEN] MUT | **高** | dry-run + 二次確認 |
| `discussions.reply(topic_id, body)` | [GEN] MUT | **高** | 預設拒絕 |
| `announcements.mark_read(id)` | [GEN] MUT | 低 | |

MCP server 啟動時須 `--allow=submit,upload,...` 白名單;CLI 模式由人類 gatekeep。

### Tier 4 — Local 維運(🔒 只 CLI)

| Tool | 狀態 | 備註 |
|---|---|---|
| `login` 🔒 | [CLI] | 互動式 |
| `logout` 🔒 | [NEW] | |
| `index.refresh(course?)` 🔒 | [NEW Phase 2] | |
| `index.status()` | [NEW Phase 2] | 這個對 AI 開放,讓它知道 cache 多新 |
| `cache.clear(scope=files\|index\|all)` 🔒 | [NEW] | |

## Phase 規劃

| Phase | 目標 | 估時 |
|---|---|---|
| 0 | 抽 `cool-tools` crate,CLI 變薄殼。已 wire 的 Tier 0–1 工具搬過去 | 1 天 |
| 1 (MCP scaffold) | `cool-mcp` crate + `cool serve` 子命令,先暴露 1–2 個 read tool 跑通 e2e | 半天 |
| 2 | 補 Tier 1 缺角:`files.search`、`announcements.show`、`pages.*`、`discussions.show`、`assignments.list` 加 bucket | 半天 |
| 3 | 補 Tier 2:`todo` / `submissions` / `calendar` / `activity` | 半天 |
| 4 | PDF text + content search:`files.get_text`、`content.search`、`index.*`(sqlite + tantivy + pdftotext) | 2–3 天 |
| 5 | Tier 3 寫工具:`assignments.submit`、`discussions.reply` | 1 天 |
| 6 | 視需要:remote MCP transport(HTTP/SSE)、REST adapter、TUI client | TBD |

> Phase 1 被提前是為了**先閉環**:MCP 跑通一個 tool 後,後面每加工具都自動雙前端可用,而不是先把工具堆到一半才接。

## 持久化 / 狀態

`~/.local/share/ntucool/`(`$XDG_DATA_HOME` 優先):

```
session.json   登入 cookies(已存在)
meta.db        sqlite — 課程/檔案 metadata 快取(Phase 4)
files/         PDF 原檔快取(Phase 4)
text/          PDF 抽出純文字(Phase 4,per-page \f 分頁)
index/         tantivy index(Phase 4)
```

CLI 跟 MCP server 共用,加 process file lock 避免同時寫 index。

## Rate limit / 風險

- Concurrency cap:read 4、download 2(已有 module download 在用)
- 429 處理:讀 `Retry-After`,沒給就 1→2→4→8s exponential + jitter,上限 30s
- 中文 query length guard:< 2 chars 不送請求
- Tier 3 寫工具 dry-run + confirmation 流程定義稍後

## 非目標(明確不做)

- Quiz tools(學術誠信)
- 直接代答作業(只給資料 / draft,不自動 submit 文字内容)
- OCR 掃描 PDF(Phase 4 第一版跳過,標 `text_path = NULL`)
- Web UI / 多用戶帳號管理
