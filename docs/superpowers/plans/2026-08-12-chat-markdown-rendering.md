# Chat Markdown Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render assistant output on the answer surface while streaming and conservatively repair the confirmed strong-emphasis boundary case without altering stored Markdown.

**Architecture:** A focused `ChatMarkdown` component owns render-only Markdown compatibility normalization and delegates parsing to the existing `react-markdown` plus `remark-gfm` stack. `CodexMessage` keeps work progress and answer content as sibling surfaces, using the same renderer for live and completed answers.

**Tech Stack:** React 19, TypeScript 5.7, react-markdown 10, remark-gfm 4, Vitest 4, GitHub Actions, systemd user service.

## Global Constraints

- Do not run tests, builds, formatters, or dependency installation locally.
- Prove every RED and GREEN transition through GitHub Actions.
- Do not add or replace Markdown parser dependencies.
- Do not modify persisted messages, conversation events, API payloads, citations, or copied source text.
- Do not normalize fenced code blocks or inline code spans.
- Preserve `.runtime/paper-codex.env`, databases, research cache, and Codex home during deployment.
- Only GitHub Actions builds release artifacts; deploy the checksum-verified `aarch64-unknown-linux-gnu` artifact.
- Final acceptance requires the deployed health endpoint to report `0.5.13`.

---

### Task 1: Specify Markdown Compatibility and Live Answer Layout

**Files:**
- Create: `web/src/ChatMarkdown.test.tsx`
- Modify: `web/src/codex-panel.test.tsx`

**Interfaces:**
- Expects: `ChatMarkdown({ children: string })` exported from `web/src/ChatMarkdown.tsx`.
- Expects: live assistant output under `.codex-markdown.conversation-live-output`, outside `.codex-worklog`.

- [ ] **Step 1: Add Markdown rendering regression tests**

Use `renderToStaticMarkup` to assert that `**结论。**目前` and `**Conclusion.**Next` produce one `<strong>` element and no literal `**`. Assert that `**结论。** 目前` stays valid, and that `` `**结论。**目前` `` plus fenced code retain literal stars inside `<code>`.

- [ ] **Step 2: Correct the live-message component contract test**

Replace the existing assertion that treats live output as part of the worklog. Render a streaming assistant message with both `live_content` and a worklog, then assert:

```ts
expect(html).toContain('class="codex-worklog"')
expect(html).toContain('class="codex-markdown conversation-live-output"')
expect(html.indexOf('class="codex-worklog"')).toBeLessThan(html.indexOf('class="codex-markdown conversation-live-output"'))
expect(html).toContain("工作过程")
expect(html).toContain("正在核对实验设置")
```

- [ ] **Step 3: Commit and push only the tests**

Commit `web/src/ChatMarkdown.test.tsx` and `web/src/codex-panel.test.tsx`, push the branch, and open a PR. Do not include production changes.

- [ ] **Step 4: Verify RED in GitHub Actions**

Run `gh pr checks <number> --watch`. Frontend CI must fail because `ChatMarkdown` does not exist and because the current live-answer structure does not meet the new contract. Rust CI should remain green.

---

### Task 2: Add the Conservative Markdown Renderer

**Files:**
- Create: `web/src/ChatMarkdown.tsx`

**Interfaces:**
- Produces: `normalizeChatMarkdown(source: string): string` for direct edge-case tests if useful.
- Produces: `ChatMarkdown({ children: string }): JSX.Element` using the existing `ReactMarkdown` and `remarkGfm` dependencies.

- [ ] **Step 1: Implement protected-region scanning**

Scan the source into normal-text, fenced-code, and inline-code regions. Apply normalization only to normal-text regions and concatenate every region without changing protected content.

- [ ] **Step 2: Implement the narrow emphasis-boundary normalization**

In normal text, recognize balanced double-asterisk spans. When the character before the candidate closing `**` is punctuation and the character after it is non-whitespace ordinary text, insert exactly one space after the delimiter for rendering. Leave unmatched, already valid, escaped, and unrelated delimiters unchanged.

- [ ] **Step 3: Render through the existing parser stack**

Return:

```tsx
<ReactMarkdown remarkPlugins={[remarkGfm]}>
  {normalizeChatMarkdown(children)}
</ReactMarkdown>
```

Do not mutate `children` or write normalized content back to state.

- [ ] **Step 4: Commit, push, and verify partial GREEN**

Push the renderer implementation. GitHub frontend CI should now pass the Markdown compatibility tests while the live-layout contract remains RED.

---

### Task 3: Separate Live Answers from Work Progress

**Files:**
- Modify: `web/src/CodexMessage.tsx`
- Modify if needed: `web/src/codex-panel.css`

**Interfaces:**
- Consumes: `ChatMarkdown` from Task 2.
- Preserves: `CodexWorklog`, `ConversationProgress`, citations, failure rendering, and message status behavior.

- [ ] **Step 1: Replace direct Markdown renderers**

Use `ChatMarkdown` for user prompts, live assistant content, and completed assistant content. Remove duplicate `ReactMarkdown` and `remarkGfm` imports from `CodexMessage.tsx`.

- [ ] **Step 2: Make live work and answer sibling surfaces**

For active statuses, render `.codex-worklog` with only work/progress content, followed by:

```tsx
{message.live_content && (
  <div className="codex-markdown conversation-live-output">
    <ChatMarkdown>{message.live_content}</ChatMarkdown>
  </div>
)}
```

Completed messages retain `.codex-markdown` and render `message.content` through `ChatMarkdown`.

- [ ] **Step 3: Adjust only layout CSS required by the sibling structure**

Keep existing typography and widths. Remove or revise selectors that assume `.conversation-live-output` is nested in `.codex-worklog`; do not redesign the conversation UI.

- [ ] **Step 4: Commit, push, and verify full GREEN**

Use `gh pr checks <number> --watch`. Frontend tests, TypeScript, frontend build, Rust formatting, Clippy, and Rust tests must all succeed in GitHub Actions.

---

### Task 4: Review, Release, and Deploy 0.5.13

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: GitHub Release `v0.5.13` and its checksums.
- Deploys: `paper-codex-v0.5.13-aarch64-unknown-linux-gnu.tar.gz`.

- [ ] **Step 1: Review scope and CI evidence**

Confirm the PR contains only the design, plan, regression tests, Markdown renderer, live-layout change, and necessary CSS. Confirm RED and GREEN runs are recorded. Do not run local executable verification.

- [ ] **Step 2: Merge and verify main CI**

Merge through GitHub and wait for the `main` CI workflow and required cross-cache workflow to succeed.

- [ ] **Step 3: Bump and publish version 0.5.13**

Change only the root package versions in `Cargo.toml` and `Cargo.lock` from `0.5.12` to `0.5.13`, commit, push, wait for CI, create tag `v0.5.13`, and wait for the GitHub Release workflow to publish all target archives and checksums.

- [ ] **Step 4: Download and checksum the ARM64 GNU artifact**

Download the archive and `.sha256` into `.runtime/downloads/v0.5.13/`, run `sha256sum -c`, and extract into a new `.runtime/releases/v0.5.13/`. Do not overwrite an existing release directory.

- [ ] **Step 5: Switch the runtime and restart the service**

Preserve `.runtime/paper-codex.env`, `paper-workspace/.paper-wiki/state.sqlite`, `.runtime/research-cache`, and `.runtime/codex-home`. Atomically repoint `.runtime/current` to `.runtime/releases/v0.5.13` and restart the existing `paper-codex.service` without replacing its installation configuration.

- [ ] **Step 6: Verify deployment acceptance**

Verify the service is active, local and public health endpoints report `version: 0.5.13`, and logs show no startup or database migration errors. Keep `v0.5.12` available as the rollback target until acceptance is complete.
