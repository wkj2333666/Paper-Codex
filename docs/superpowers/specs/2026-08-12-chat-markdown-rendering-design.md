# Chat Markdown Rendering Design

## Scope

This change fixes two confirmed conversation rendering defects:

1. The assistant's streaming answer is rendered inside the work-process panel until the turn completes.
2. Strong emphasis is occasionally displayed as literal `**` when a closing delimiter follows punctuation and is immediately followed by ordinary text, for example `**结论。**目前`.

It does not change Codex prompts, persisted message content, conversation events, citations, or the Markdown syntax accepted outside this narrowly identified compatibility case.

## Confirmed Causes

`CodexMessage` deliberately nests `message.live_content` inside `.codex-worklog`. The backend already separates answer deltas from work-summary deltas, so the defect is entirely in the frontend component structure.

The affected completed message is valid and identical in the raw Codex session, `answer-completed` event, and persisted database content. The literal stars result from CommonMark delimiter rules: in `**内容。**目前`, the candidate closing delimiter is preceded by punctuation and followed by non-whitespace ordinary text, so a strict parser does not recognize it as a closing strong-emphasis delimiter. This explains why the failure is final-state, data-dependent, and intermittent.

## Rendering Architecture

A focused `ChatMarkdown` component becomes the single Markdown renderer used by user prompts, live assistant answers, completed assistant answers, and work summaries where applicable. Before passing source text to `react-markdown`, it applies a conservative compatibility normalization:

- Detect a balanced `**...**` span whose closing delimiter is rejected only because punctuation immediately before the delimiter is followed immediately by ordinary text.
- Insert one render-only space after that closing delimiter.
- Leave already valid emphasis unchanged.
- Leave unmatched delimiters unchanged.
- Do not normalize inside fenced code blocks or inline code spans.
- Do not modify the message object, database content, events, citations, copy source, or API payloads.

The implementation remains based on `react-markdown` and `remark-gfm`; no parser dependency is replaced or added.

## Live Answer Layout

`CodexMessage` renders two sibling surfaces for an active assistant turn:

1. The work-process surface contains `CodexWorklog` or `ConversationProgress` only.
2. When `live_content` is non-empty, the normal answer surface renders it with `ChatMarkdown` outside `.codex-worklog`.

On `answer-completed`, the existing reducer replaces the preview with authoritative `content`, and the same normal answer surface renders the completed text. Citation cards remain below the answer as they are now.

## Testing

Frontend regression tests cover:

- `**结论。**目前` renders as `<strong>结论。</strong>` followed by `目前` without literal stars.
- Equivalent ASCII punctuation is normalized.
- Already valid emphasis remains unchanged.
- Fenced code and inline code retain their literal source text.
- A live answer appears in the normal Markdown answer surface and not inside `.codex-worklog`.
- The work-process surface remains visible independently during a live answer.
- A completed answer continues to render citations and uses the same Markdown compatibility behavior.

Per project policy, no tests, builds, formatters, or dependency installation run locally. RED and GREEN are proven in GitHub Actions. Release artifacts are built only by GitHub Actions. Deployment downloads and checksum-verifies the `aarch64-unknown-linux-gnu` artifact while preserving `.runtime/paper-codex.env`, databases, research cache, and Codex home. Completion requires the deployed health endpoint to report version `0.5.13`.
