# Paper Codex Search–Select–Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make free-text paper input show ranked, inspectable candidates before import, while DOI/arXiv/URL inputs continue to import directly and selected candidates use robust multi-source PDF acquisition.

**Architecture:** Extract project-independent discovery from `ResearchService`, expose it through a dedicated intake-search API, and protect the existing intake API from free-text imports. Persist aggregated provider/source metadata with each discovered work, then create an ingest task from a server-built candidate snapshot. Keep `TaskEngine` as the sole import/analyze queue and expose structured source failures to the current task cards.

**Tech Stack:** Rust, Axum, SQLite/sqlx, reqwest, React, TypeScript, Vitest, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-01-intake-search-selection-design.md`

## Global Constraints

- Free text always enters search and always requires explicit candidate confirmation, including exact titles and single-result searches.
- DOI, arXiv identifiers/URLs, ordinary URLs, OpenReview URLs, and direct PDF URLs continue to create intake tasks immediately.
- The Rust `classify_input` result is authoritative. Frontend heuristics may change presentation only and may not decide which backend operation is permitted.
- Candidate import accepts only `work_id` plus optional `project_id`; title, identifiers, and download URLs come from server-side persisted discovery data.
- Do not bypass OpenReview browser challenges, scrape general search-engine HTML, or add a required paid/API-key provider.
- Do not put API keys, cookies, authorization headers, or response HTML in task errors, events, logs, or frontend state.
- Do not run builds, Cargo tests, Clippy, npm tests, TypeScript checks, or frontend builds locally. Run the full verification suite only through GitHub Actions.
- Locally, only source inspection, `cargo fmt --all`, and `git diff --check` are permitted.
- Each implementation batch is committed and pushed before its GitHub CI result is used as evidence. A failed CI job is repaired from its remote log rather than reproduced locally.

---

### Task 1: Extract Reusable Discovery and Deterministic Ranking

**Files:**
- Modify: `src/research.rs`
- Modify: `src/research_service.rs`
- Modify: `src/research_store.rs`
- Test: `tests/research_service.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryMatch {
    pub score: f64,
    pub title_exact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulltextState {
    Available,
    Possible,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryFulltext {
    pub state: FulltextState,
    pub source_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryResult {
    pub work: DiscoveredWork,
    pub providers: Vec<String>,
    pub best_rank: Option<i64>,
    pub provider_scores: serde_json::Value,
    pub raw_results: Vec<serde_json::Value>,
    pub match_info: DiscoveryMatch,
    pub fulltext: DiscoveryFulltext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryOutcome {
    pub state: SearchRunState,
    pub provider_status: serde_json::Value,
    pub results: Vec<DiscoveryResult>,
}

impl ResearchService {
    pub async fn discover(
        &self,
        query: ResearchQuery,
        cancel: watch::Receiver<bool>,
    ) -> Result<DiscoveryOutcome>;
}
```

- [ ] **Step 1: Add discovery regression tests** using fake providers: one provider succeeds and one fails (`Partial`), all succeed (`Completed`), all fail (typed aggregate error), zero hits (successful empty result), cancellation, and DOI/arXiv/OpenAlex duplicate coalescing.
- [ ] **Step 2: Add ranking fixtures** for `jepa`, an exact title, and `Yann LeCun. A Path Towards Autonomous Machine Intelligence... 2022`; assert normalized exact title, title-token coverage, author/year agreement, multi-provider agreement, and provider rank determine a stable order.
- [ ] **Step 3: Implement pure ranking helpers** in `research_service.rs`. Normalize Unicode case and punctuation, retain four-digit years, extract author-like tokens only as tie-breakers, clamp scores to `0.0..=1.0`, and use canonical work ID as the final deterministic tie-break.
- [ ] **Step 4: Extract provider fan-out and merge logic** from `search_with_cancel` into `discover`. `discover` must not create a project, conversation, message, `literature_search_run`, or project candidate.
- [ ] **Step 5: Keep project research behavior unchanged** by making `search_with_cancel` call `discover`, then persist the existing search run/results/candidates from the returned outcome.
- [ ] **Step 6: Persist source aggregation in work metadata** under the reserved server-owned key `_paper_codex`:

```json
{
  "_paper_codex": {
    "providers": ["arxiv", "crossref"],
    "pdf_sources": [
      {"provider": "arxiv", "url": "https://arxiv.org/pdf/2301.08243"}
    ]
  }
}
```

  Merge this key during `ResearchStore::upsert_work` without discarding provider raw metadata, deduplicate normalized URLs, and never accept `_paper_codex` values from an intake client.
- [ ] **Step 7: Add store tests** proving repeated discoveries merge provider names/PDF sources and preserve prior sources when a later provider response omits a PDF.
- [ ] **Step 8: Format and inspect only** with `cargo fmt --all` and `git diff --check`; do not run test binaries locally.
- [ ] **Step 9: Commit** with `feat: extract reusable paper discovery`.

### Task 2: Add Intake Search API and Protect Direct Intake

**Files:**
- Modify: `src/api.rs`
- Modify: `src/acquisition.rs` only to expose/reuse authoritative input classification
- Test: `tests/api.rs`
- Test: `tests/acquisition.rs`

**Interfaces:**

```rust
#[derive(Deserialize)]
struct IntakeSearchRequest {
    query: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct IntakeSearchResponse {
    query: String,
    state: SearchRunState,
    providers: serde_json::Value,
    results: Vec<DiscoveryResult>,
}
```

- `POST /api/intake/search`
- `POST /api/intake` returns `{"kind":"enqueued","task_id":"..."}` for direct inputs.
- Free text sent to `POST /api/intake` returns HTTP 422 with `{"code":"intake_search_required","error":"..."}`.
- Search with all providers failed returns HTTP 503 with `code = "paper_search_failed"` and the provider-status object.

- [ ] **Step 1: Add classification table tests** for DOI forms, arXiv ID/forms, arXiv URL, OpenReview URL, PDF URL, ordinary URL, short search term, title, and full natural-language citation.
- [ ] **Step 2: Extend `ApiError` with an optional stable code and JSON details** while preserving existing `{"error": message}` compatibility for all older errors.
- [ ] **Step 3: Add API regression tests** proving free text can no longer create a task, all direct input kinds still enqueue, blank query is 400, limit defaults to 12, and values outside `1..=25` are 400.
- [ ] **Step 4: Implement `/api/intake/search`** using `AppState.research`; return 503 when research is unavailable, pass a non-cancelled watch receiver to `discover`, and serialize partial provider failures without failing the whole request when at least one provider succeeded.
- [ ] **Step 5: Change `/api/intake`** to call the authoritative classifier before `create_ingest`; add the tagged `kind` field without removing `task_id`.
- [ ] **Step 6: Verify no search request creates task/task-event rows** and no direct intake request creates a literature-search run.
- [ ] **Step 7: Format and inspect only** with `cargo fmt --all` and `git diff --check`.
- [ ] **Step 8: Commit** with `feat: add intake paper search endpoint`.

### Task 3: Build Server-Side Candidate Snapshots

**Files:**
- Modify: `src/research.rs`
- Modify: `src/research_store.rs`
- Modify: `src/research_service.rs`
- Modify: `src/tasks.rs`
- Test: `tests/research_service.rs`
- Test: `tests/tasks.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdfSource {
    pub provider: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateSnapshot {
    pub work_id: String,
    pub canonical_key: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub source_url: String,
    pub pdf_sources: Vec<PdfSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestInput {
    pub source: String,
    pub project_id: Option<String>,
    pub upload_path: Option<PathBuf>,
    #[serde(default)]
    pub bulk_import: bool,
    #[serde(default)]
    pub candidate: Option<CandidateSnapshot>,
}
```

```rust
impl ResearchService {
    pub async fn candidate_snapshot(&self, work_id: &str) -> Result<CandidateSnapshot>;
}
```

- [ ] **Step 1: Add snapshot tests** for a work with provider PDF, arXiv ID, DOI, and OpenAlex/Crossref source metadata; assert source ordering and URL deduplication.
- [ ] **Step 2: Add negative tests** for unknown work ID and a work without source URL, identifier, or PDF source; map the latter to `candidate_not_importable`.
- [ ] **Step 3: Implement `ResearchStore::get_work` metadata hydration** so `_paper_codex.pdf_sources` is server-read and schema-validated; ignore malformed entries instead of failing an otherwise valid work.
- [ ] **Step 4: Implement source ordering:** provider-declared direct PDF first, canonical arXiv PDF second, DOI-resolved Crossref PDF third, OpenAlex OA PDF fourth. Deduplicate normalized URLs while preserving first-seen priority.
- [ ] **Step 5: Extend `IngestInput` compatibly** with `#[serde(default)]`; update every constructor to set `candidate: None` and add a serialization test proving historical task JSON still deserializes.
- [ ] **Step 6: Update ingest execution** so a candidate snapshot supplies resolved paper metadata and bypasses `Acquirer::resolve_title`; direct DOI/arXiv/URL and upload paths retain their current behavior.
- [ ] **Step 7: Add a task regression test** in which candidate `source` is misleading free text but the snapshot is correct; assert the acquirer never performs title resolution and the stored paper uses snapshot metadata.
- [ ] **Step 8: Format and inspect only** with `cargo fmt --all` and `git diff --check`.
- [ ] **Step 9: Commit** with `feat: ingest confirmed paper candidates`.

### Task 4: Add Multi-Source PDF Acquisition and Structured Failures

**Files:**
- Modify: `src/acquisition.rs`
- Modify: `src/domain.rs`
- Modify: `src/db.rs`
- Modify: `src/tasks.rs`
- Test: `tests/acquisition.rs`
- Test: `tests/tasks.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadAttempt {
    pub provider: String,
    pub url: String,
    pub status: Option<u16>,
    pub reason_code: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskFailureDetails {
    pub code: String,
    #[serde(default)]
    pub attempts: Vec<DownloadAttempt>,
}

pub async fn download_pdf_sources(
    &self,
    sources: &[PdfSource],
    cancel: watch::Receiver<bool>,
) -> Result<(Vec<u8>, PdfSource), AllPdfSourcesFailed>;
```

- Add nullable `tasks.error_details_json`; fresh schema and migration paths must both create it.
- `Task` adds `error_details_json: Option<String>` so task-list responses preserve failure details after reload.

- [ ] **Step 1: Add local test-server fixtures** for a 403 challenge body, 404, invalid PDF body, transient 503 followed by success, and a second-source success after the first source is exhausted.
- [ ] **Step 2: Add redaction tests** proving bearer tokens, cookie values, request headers, and raw HTML never appear in `DownloadAttempt` or `tasks.error`.
- [ ] **Step 3: Refactor single-source download errors** to retain status/reason without calling `error_for_status` before reading a bounded error body. Read at most 16 KiB for classification and discard the body afterward.
- [ ] **Step 4: Detect OpenReview challenge failures** when status is 403 and the bounded body contains `Challenge verification required`; emit `reason_code = "browser_challenge_required"` with the approved Chinese message.
- [ ] **Step 5: Implement sequential source fallback**. Keep the existing bounded retry policy inside each source; after that source is exhausted, append one final attempt record and continue to the next distinct source.
- [ ] **Step 6: Return `AllPdfSourcesFailed` only after every source fails**. Its user-facing summary is `已定位论文，但所有 PDF 来源均失败`; it exposes structured attempts through a typed accessor, not by embedding JSON in `Display`.
- [ ] **Step 7: Persist failure details and emit them** in the failed task event payload:

```json
{
  "message": "已定位论文，但所有 PDF 来源均失败",
  "details": {
    "code": "all_pdf_sources_failed",
    "attempts": []
  }
}
```

- [ ] **Step 8: Add database migration/idempotence tests** and task reload tests for `error_details_json`.
- [ ] **Step 9: Format and inspect only** with `cargo fmt --all` and `git diff --check`.
- [ ] **Step 10: Commit** with `fix: add robust pdf source fallback`.

### Task 5: Add Candidate Import API

**Files:**
- Modify: `src/api.rs`
- Modify: `src/research_service.rs`
- Modify: `src/tasks.rs`
- Test: `tests/api.rs`

**Interfaces:**

```rust
#[derive(Deserialize)]
struct CandidateImportRequest {
    project_id: Option<String>,
}
```

- `POST /api/intake/candidates/{work_id}/import`
- Success union:

```json
{"state":"enqueued","task_id":"task-id"}
```

```json
{"state":"existing","paper_id":"paper-id"}
```

- [ ] **Step 1: Add API tests** for unknown work (404), non-importable work (409 with `candidate_not_importable`), new candidate enqueue, existing paper return, and optional project membership for an existing paper.
- [ ] **Step 2: Implement candidate identity lookup** using canonical key, DOI, then arXiv ID; do not match by title alone.
- [ ] **Step 3: Implement the existing-paper branch**: create missing project membership idempotently and return the canonical paper ID without a task or download.
- [ ] **Step 4: Implement the enqueue branch** by calling `candidate_snapshot`, constructing `IngestInput { candidate: Some(...) }`, and creating the normal ingest task.
- [ ] **Step 5: Verify request JSON cannot override metadata**: extra client fields such as `title`, `doi`, and `pdf_url` are ignored or rejected consistently, and only the stored work is used.
- [ ] **Step 6: Add a regression test** proving the selected LeCun work cannot be replaced by Crossref's first title-search result after the user clicks import.
- [ ] **Step 7: Format and inspect only** with `cargo fmt --all` and `git diff --check`.
- [ ] **Step 8: Commit** with `feat: add confirmed candidate import endpoint`.

### Task 6: Add the Home Candidate Selection UI

**Files:**
- Modify: `web/src/types.ts`
- Modify: `web/src/api.ts`
- Create: `web/src/IntakeSearchResults.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`
- Test: `web/src/api.test.ts`
- Create: `web/src/IntakeSearchResults.test.tsx`
- Modify: `web/src/workbench-intake.test.tsx`

**Interfaces:**

```ts
export type IntakeSearchResponse = {
  query: string
  state: "completed" | "partial"
  providers: Record<string, ProviderStatus>
  results: IntakeSearchResult[]
}

export type CandidateImportResult =
  | { state: "enqueued"; task_id: string }
  | { state: "existing"; paper_id: string }
```

```ts
api.searchIntake(query: string, limit?: number): Promise<IntakeSearchResponse>
api.importIntakeCandidate(workId: string, projectId?: string): Promise<CandidateImportResult>
```

- [ ] **Step 1: Add API client tests** for method, path, JSON body, tagged response decoding, 422 stable error-code decoding, and partial-provider response preservation.
- [ ] **Step 2: Implement frontend types** by reusing the existing `DiscoveredWork` and `ProviderStatus` types; do not create a second incompatible work schema.
- [ ] **Step 3: Implement `IntakeSearchResults`** with loading skeleton, provider partial-warning strip, no-results state, candidate rows/cards, weak-match label, fulltext state, provider badges, and per-candidate import busy/error state.
- [ ] **Step 4: Keep the candidate panel compact and responsive** in the existing intake card flow. Each result shows title, up to three authors plus overflow count, year, identifiers, provider names, and one explicit `导入并分析` action.
- [ ] **Step 5: Update `Workbench.submit`** so it first calls the server-authoritative direct intake endpoint; on `intake_search_required`, call `searchIntake` and show candidates without creating/refreshing task cards. Do not duplicate the Rust classifier in TypeScript.
- [ ] **Step 6: On candidate import** refresh the dashboard for `enqueued`; for `existing`, refresh then call `select({kind:"paper", id:paper_id})`. Preserve the selected project in both paths.
- [ ] **Step 7: Preserve typed query text** while candidates are displayed; clear it only after successful direct enqueue or confirmed candidate import. A failed search/import must leave both query and candidate list available for retry.
- [ ] **Step 8: Add component tests** for `jepa` candidate display, full citation candidate display, partial provider failure, zero results, import failure, existing-paper navigation, and no silent import when exactly one candidate is returned.
- [ ] **Step 9: Add Workbench regression tests** proving direct arXiv input still produces a task and free text does not appear in “正在处理” or “最近失败” until a candidate is selected.
- [ ] **Step 10: Inspect with `git diff --check` only**; do not run Vitest, TypeScript, or Vite locally.
- [ ] **Step 11: Commit** with `feat: add intake candidate selection ui`.

### Task 7: Render Structured Download Failures in Task Cards

**Files:**
- Modify: `web/src/types.ts`
- Modify: `web/src/intake-status.ts`
- Modify: `web/src/IntakeTaskCard.tsx`
- Modify: `web/src/styles.css`
- Modify: `web/src/IntakeTaskCard.test.tsx`

- [ ] **Step 1: Add typed `TaskFailureDetails` and `DownloadAttempt` parsing**. Malformed or absent JSON falls back to the existing plain `task.error` display.
- [ ] **Step 2: Add card tests** for browser challenge, multiple failed sources, status-less transport error, malformed details, and redacted URLs/messages.
- [ ] **Step 3: Render a concise summary by default** and a native `<details>` section labeled `查看 N 个来源尝试`; each row shows provider, safe hostname/path, HTTP status when available, and translated reason.
- [ ] **Step 4: For `browser_challenge_required`**, show the explicit browser-verification explanation and a safe external source link only when the stored URL uses `http` or `https`.
- [ ] **Step 5: Keep old tasks compatible**: tasks with only `error` render exactly as before, and cancelled/active task controls are unchanged.
- [ ] **Step 6: Inspect with `git diff --check` only**; do not run frontend tests locally.
- [ ] **Step 7: Commit** with `feat: show paper source failure details`.

### Task 8: Remote CI Integration, Release, and Deployment

**Files:**
- Modify: `.github/workflows/ci.yml` only if existing jobs do not already collect every new Rust/frontend test target
- Modify: release notes/version files only as required by the repository's existing tag workflow

- [ ] **Step 1: Run final local non-build checks**: `cargo fmt --all` and `git diff --check`. Record that no local build, test, typecheck, or Clippy command was run.
- [ ] **Step 2: Push the branch and open/update the pull request** with the approved design, implementation-plan link, behavior matrix, schema migration, security/redaction notes, and manual acceptance cases.
- [ ] **Step 3: Let GitHub Actions run all verification**: Rust format, Clippy, all Rust tests, frontend Vitest, TypeScript, frontend build, and release-build checks already defined by CI.
- [ ] **Step 4: If CI fails**, inspect the GitHub job log, make the smallest source/test correction, format/check diff locally, push, and wait for the replacement CI run. Do not reproduce the failure through local builds/tests.
- [ ] **Step 5: After CI is green, merge the PR** into `main` using the repository's current merge policy.
- [ ] **Step 6: Create the next release tag** from the merged `main` commit and wait for the tag-based GitHub release workflow to publish both architecture artifacts.
- [ ] **Step 7: Download the published `aarch64` artifact** rather than compiling locally. Verify its release checksum/signature if the workflow publishes one.
- [ ] **Step 8: Deploy with the existing release-directory/current-symlink procedure**, preserve the database and configuration, restart `paper-codex.service`, and retain the previous release for rollback.
- [ ] **Step 9: Perform bounded production smoke checks**: service active, homepage HTTP 200, `jepa` shows candidates without a task, the full LeCun citation shows the intended candidate, `arxiv:2301.08243` enqueues directly, candidate import creates/opens the correct paper, and a simulated/known OpenReview 403 displays the structured browser-challenge reason.
- [ ] **Step 10: Report the merged commit, release tag, deployed artifact, service status, and smoke-check results.**
