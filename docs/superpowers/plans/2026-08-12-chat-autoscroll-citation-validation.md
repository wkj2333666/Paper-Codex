# Chat Autoscroll and Candidate Citation Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep loaded and actively streaming conversations at their newest content when the user wants follow mode, and prevent harmless model title formatting from invalidating inspected candidate citations.

**Architecture:** The backend treats `CandidateSource` as authoritative for the display title after preserving all source identity, evidence strength, and content safety checks. The frontend adds a small framework-independent scroll controller and connects it to the existing conversation feed with a content wrapper, a resize observer, and an immediate first-load layout effect.

**Tech Stack:** Rust 1.91.1, anyhow, React 19, TypeScript 5.7, Vitest 4, GitHub Actions, systemd user service.

## Global Constraints

- Do not run tests, builds, formatters, or dependency installation locally.
- Prove RED and GREEN through GitHub Actions on the pull request.
- Preserve `.runtime/paper-codex.env`, the workspace database, research cache, and isolated Codex home.
- Only GitHub Actions may build release artifacts; this host only downloads, verifies, and deploys the `aarch64-unknown-linux-gnu` artifact.
- Source URL mismatch, unknown inspected work, duplicate citation IDs, inflated evidence level, empty quotes, and oversized text remain hard validation failures.
- The bottom threshold is exactly `24px`.

---

### Task 1: Normalize Inspected Candidate Titles

**Files:**
- Modify: `src/prompts.rs:162-187`
- Test: `src/prompts.rs:385-539`

**Interfaces:**
- Consumes: `CandidateSource { work_id, title, source_url, evidence_level, .. }` keyed by `work_id` in `candidate_sources`.
- Produces: `validate_conversation_answer_with_candidates(...) -> Result<ConversationAnswer>` with every accepted candidate citation title copied from its inspected `CandidateSource`.

- [ ] **Step 1: Add the backend regression tests before implementation**

Add a reusable inspected candidate fixture and three tests in the existing `prompts.rs` test module:

```rust
fn inspected_candidate() -> CandidateSource {
    CandidateSource {
        work_id: "work-1".into(),
        title: "DINOv2: Learning Robust Visual Features without Supervision".into(),
        source_url: "https://arxiv.org/abs/2304.07193".into(),
        evidence_level: EvidenceLevel::Abstract,
        abstract_text: Some("Robust visual features.".into()),
        pdf_url: None,
    }
}

#[test]
fn inspected_candidate_uses_authoritative_title() {
    let mut evidence = HashMap::new();
    evidence.insert("work-1".into(), inspected_candidate());
    let normalized = validate_conversation_answer_with_candidates(
        candidate_answer("DINO v2"), "问题", &[], &evidence,
    ).unwrap();
    assert_eq!(normalized.candidate_citations[0].title, inspected_candidate().title);
}

#[test]
fn inspected_candidate_rejects_source_url_mismatch() {
    let mut evidence = HashMap::new();
    evidence.insert("work-1".into(), inspected_candidate());
    let mut answer = candidate_answer("DINO v2");
    answer.candidate_citations[0].source_url = "https://example.com/wrong".into();
    let error = validate_conversation_answer_with_candidates(
        answer, "问题", &[], &evidence,
    ).unwrap_err();
    assert_eq!(
        error.to_string(),
        "candidate citation source URL does not match inspected evidence",
    );
}

#[test]
fn inspected_candidate_rejects_inflated_evidence_level() {
    let mut evidence = HashMap::new();
    evidence.insert("work-1".into(), inspected_candidate());
    let mut answer = candidate_answer("DINO v2");
    answer.candidate_citations[0].evidence_level = EvidenceLevel::Fulltext;
    let error = validate_conversation_answer_with_candidates(
        answer, "问题", &[], &evidence,
    ).unwrap_err();
    assert_eq!(
        error.to_string(),
        "candidate citation claims stronger evidence than was inspected",
    );
}
```

The `candidate_answer` fixture must provide one valid, unique candidate citation and no local citations or annotation intents.

- [ ] **Step 2: Commit and push only the tests, then open the PR**

```bash
git add src/prompts.rs
git commit -m "test: cover inspected candidate title normalization"
git push -u origin codex/chat-autoscroll-citation-validation
gh pr create --base main --head codex/chat-autoscroll-citation-validation --title "Fix chat following and candidate citation validation" --body "Fix candidate citation title normalization and add bottom-aware conversation follow scrolling. All verification runs in GitHub Actions; no local builds or tests."
```

- [ ] **Step 3: Verify RED in GitHub Actions**

Run:

```bash
PR_NUMBER=$(gh pr view --json number --jq .number)
gh pr checks "$PR_NUMBER" --watch
```

Expected: Rust checks fail specifically because the validator still returns `candidate citation title does not match inspected evidence`; frontend checks remain green.

- [ ] **Step 4: Implement the minimal backend normalization**

Keep the exact source URL and evidence-level checks, remove only the fatal title equality check, and assign the inspected title after those identity checks:

```rust
if citation.source_url != inspected.source_url {
    bail!("candidate citation source URL does not match inspected evidence");
}
if inspected.evidence_level.strongest(citation.evidence_level) != inspected.evidence_level {
    bail!("candidate citation claims stronger evidence than was inspected");
}
citation.title = clean_control_characters(&inspected.title);
```

- [ ] **Step 5: Commit, push, and verify GREEN in GitHub Actions**

```bash
git add src/prompts.rs
git commit -m "Fix inspected candidate title normalization"
git push
PR_NUMBER=$(gh pr view --json number --jq .number)
gh pr checks "$PR_NUMBER" --watch
```

Expected: both frontend and Rust checks succeed with zero failed jobs.

---

### Task 2: Add the Conversation Scroll Controller

**Files:**
- Create: `web/src/conversation-scroll.ts`
- Create: `web/src/conversation-scroll.test.ts`

**Interfaces:**
- Produces: `BOTTOM_THRESHOLD_PX = 24`.
- Produces: `ConversationScrollViewport` with `scrollHeight`, `scrollTop`, `clientHeight`, and `scrollTo({ top, behavior })`.
- Produces: `isConversationAtBottom(viewport, threshold?) -> boolean`.
- Produces: `ConversationScrollController` with `reset()`, `handleScroll()`, `positionInitial()`, `followContent()`, and `isPinned()`.

- [ ] **Step 1: Write the failing controller tests**

The tests use a plain fake viewport, not DOM mocks:

```ts
const viewport = (remaining: number) => {
  const calls: ScrollToOptions[] = []
  return {
    calls,
    value: {
      scrollHeight: 1000,
      clientHeight: 400,
      scrollTop: 600 - remaining,
      scrollTo: (options: ScrollToOptions) => calls.push(options),
    },
  }
}

it("uses a 24px bottom threshold", () => {
  expect(isConversationAtBottom(viewport(24).value)).toBe(true)
  expect(isConversationAtBottom(viewport(25).value)).toBe(false)
})

it("pauses after scrolling up and resumes at the bottom", () => {
  const fake = viewport(0)
  const controller = new ConversationScrollController(() => fake.value)
  controller.handleScroll()
  fake.value.scrollTop = 500
  controller.handleScroll()
  controller.followContent()
  expect(fake.calls).toHaveLength(0)
  fake.value.scrollTop = 600
  controller.handleScroll()
  controller.followContent()
  expect(fake.calls.at(-1)).toEqual({ top: 1000, behavior: "smooth" })
})

it("positions a loaded conversation immediately", () => {
  const fake = viewport(100)
  const controller = new ConversationScrollController(() => fake.value)
  controller.positionInitial()
  expect(fake.calls).toEqual([{ top: 1000, behavior: "auto" }])
  expect(controller.isPinned()).toBe(true)
})
```

- [ ] **Step 2: Commit and push only the frontend tests**

```bash
git add web/src/conversation-scroll.test.ts
git commit -m "test: specify conversation follow scrolling"
git push
```

- [ ] **Step 3: Verify RED in GitHub Actions**

Set `PR_NUMBER=$(gh pr view --json number --jq .number)`, then run `gh pr checks "$PR_NUMBER" --watch`.

Expected: frontend checks fail because `./conversation-scroll` does not exist; Rust checks stay green.

- [ ] **Step 4: Implement the minimal controller**

Create `conversation-scroll.ts` with no React dependency. Clamp the remaining distance to the formula from the design, keep pinned state private, use `auto` for `positionInitial`, and call `scrollTo` only from `followContent` when pinned.

- [ ] **Step 5: Commit, push, and verify GREEN in GitHub Actions**

```bash
git add web/src/conversation-scroll.ts
git commit -m "Add conversation scroll controller"
git push
PR_NUMBER=$(gh pr view --json number --jq .number)
gh pr checks "$PR_NUMBER" --watch
```

Expected: both CI jobs succeed.

---

### Task 3: Connect Follow Scrolling to the Conversation Feed

**Files:**
- Modify: `web/src/CodexPanel.tsx:1-190`
- Modify: `web/src/codex-panel.test.tsx`

**Interfaces:**
- Consumes: `ConversationScrollController` from Task 2.
- Uses: one `conversation-feed` viewport ref and one `conversation-feed-content` resize target ref.
- Preserves: the existing `.conversation-feed` as the sole scroll container.

- [ ] **Step 1: Add static integration assertions before production wiring**

Extend the existing server-rendered panel test to assert that the rendered panel contains a feed content wrapper and bottom sentinel:

```ts
expect(html).toContain('class="conversation-feed"')
expect(html).toContain('class="conversation-feed-content"')
expect(html).toContain('data-testid="conversation-bottom"')
```

- [ ] **Step 2: Commit and push only the integration test**

```bash
git add web/src/codex-panel.test.tsx
git commit -m "test: require conversation scroll anchors"
git push
```

- [ ] **Step 3: Verify RED in GitHub Actions**

Set `PR_NUMBER=$(gh pr view --json number --jq .number)`, then run `gh pr checks "$PR_NUMBER" --watch`.

Expected: frontend checks fail on the missing wrapper or sentinel assertion; Rust checks remain green.

- [ ] **Step 4: Wire the controller into `CodexPanel`**

Implement these connections:

```tsx
const feedRef = useRef<HTMLDivElement|null>(null)
const feedContentRef = useRef<HTMLDivElement|null>(null)
const scrollControllerRef = useRef(new ConversationScrollController(() => feedRef.current))
const positionedConversationRef = useRef<string|null>(null)
```

- On `activeConversationId` change, call `reset()` and clear `positionedConversationRef` for the new conversation.
- In a layout effect after messages render, call `positionInitial()` exactly once when the active conversation has at least one rendered message.
- Attach `onScroll={() => scrollControllerRef.current.handleScroll()}` to `.conversation-feed`.
- Wrap message or empty-state content in `.conversation-feed-content`, followed by `<span data-testid="conversation-bottom" aria-hidden="true" />`.
- Observe `.conversation-feed-content` with `ResizeObserver`; its callback calls `followContent()`.
- If `ResizeObserver` is unavailable, use an effect keyed by the rendered message state to call `followContent()`.
- Disconnect the observer during cleanup.

- [ ] **Step 5: Commit, push, and verify the full PR**

```bash
git add web/src/CodexPanel.tsx web/src/codex-panel.test.tsx
git commit -m "Follow active conversation output at the bottom"
git push
PR_NUMBER=$(gh pr view --json number --jq .number)
gh pr checks "$PR_NUMBER" --watch
```

Expected: frontend tests, TypeScript, frontend build, Rust formatting, Clippy, and all Rust tests succeed.

---

### Task 4: Review, Merge, Release, and Deploy

**Files:**
- Modify after merge: `Cargo.toml` package version from `0.5.11` to `0.5.12`.
- Modify after merge: `Cargo.lock` root `paper-codex` package version from `0.5.11` to `0.5.12`.

**Interfaces:**
- Produces: GitHub Release `v0.5.12` with 16 assets.
- Deploys: `paper-codex-v0.5.12-aarch64-unknown-linux-gnu.tar.gz` after matching `.sha256` verification.

- [ ] **Step 1: Review the final diff and CI evidence**

Confirm the PR contains only the design, plan, tests, backend normalization, scroll controller, and panel wiring. Confirm both PR checks are successful. Do not run local formatters, tests, or builds.

- [ ] **Step 2: Merge the PR and verify `main` CI**

Merge through GitHub, update local `main` without rewriting user changes, and wait for the `main` CI workflow to succeed.

- [ ] **Step 3: Bump the release version on `main`**

Edit only the two package version declarations to `0.5.12`, commit, push, and wait for both `CI` and `Cross cache` workflows to succeed.

- [ ] **Step 4: Tag and publish from GitHub Actions**

Create and push tag `v0.5.12`. Wait for `release.yml` to complete successfully, then verify the release is neither draft nor prerelease and contains exactly 16 assets.

- [ ] **Step 5: Download, verify, and deploy the Raspberry Pi artifact**

Download only the GNU ARM64 archive and matching checksum into `.runtime/downloads/v0.5.12`. Run `sha256sum -c`, extract with `--strip-components=1` into `.runtime/releases/v0.5.12`, verify the executable and `web/index.html`, and atomically switch `.runtime/current` to `releases/v0.5.12`.

- [ ] **Step 6: Restart and independently verify completion**

Run `systemctl --user restart paper-codex.service`. Fresh verification must show:

```text
systemctl --user is-active paper-codex.service => active
/proc/<MainPID>/exe => .runtime/releases/v0.5.12/paper-codex
GET /api/health => {"codex":true,"status":"ok","version":"0.5.12"}
git status --short --branch => clean main tracking origin/main
git rev-list --left-right --count HEAD...origin/main => 0 0
GitHub Release v0.5.12 => 16 assets
```
