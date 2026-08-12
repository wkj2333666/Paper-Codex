# Chat Autoscroll and Candidate Citation Validation Design

## Scope

This change addresses two conversation reliability issues:

1. A loaded conversation should open at its newest content and continue following Codex output only while the user remains at the bottom.
2. A verified external candidate citation should not fail the entire answer merely because the model restates its title with harmless formatting differences.

It does not change conversation scope ownership, cancel turns when navigating between projects, or weaken source identity and evidence-level checks.

## Confirmed Failure Cause

Leaving a project page aborts only the browser's event-stream subscription. The backend turn continues unless the dedicated conversation cancel endpoint is called. The observed failure happened after Codex completed its turn, when structured candidate citations were validated.

Candidate citations are keyed by `work_id` and backed by evidence captured during the current turn's `research_inspect`. The validator currently requires the model-provided title to be byte-for-byte identical to the inspected title. Differences in whitespace, capitalization, punctuation, or subtitle formatting therefore reject the complete answer even though the referenced inspected work is unambiguous.

## Conversation Scroll Behavior

The conversation feed remains the only scroll container. A small, independently tested helper defines whether it is pinned to the bottom using the remaining scroll distance:

`scrollHeight - scrollTop - clientHeight <= 24px`

The panel maintains a mutable pinned state so high-frequency streaming updates do not cause React render loops.

- When a conversation detail is first installed or a different conversation is selected, the feed scrolls directly to the bottom after the DOM has rendered the loaded messages. This initial positioning is immediate, not animated.
- The feed's `scroll` event updates the pinned state. Moving more than 24px from the bottom pauses following. Returning within the threshold resumes it.
- While pinned, message additions and height changes caused by answer deltas, work summaries, plans, progress labels, completion, failure, or cancellation move the feed to the bottom.
- Streaming follow uses the existing smooth scroll behavior. It must not force the user down after the user has scrolled upward.
- Empty conversations do not require special scrolling.

A bottom sentinel and `ResizeObserver` are preferred over depending only on message count. Message count does not change for answer deltas or expanding work logs, while observing the rendered feed content captures both incremental text and layout-height changes. A guarded fallback performs the same bottom positioning when `ResizeObserver` is unavailable.

## Candidate Citation Normalization

For every candidate citation whose `work_id` exists in the current turn's inspected evidence map:

- Continue requiring a non-empty unique citation ID.
- Continue requiring the exact inspected source URL.
- Continue rejecting evidence levels stronger than the inspected evidence.
- Continue validating quote and explanation presence and length.
- Replace the model-provided title with the inspected evidence title before persistence and display.

The inspected title is authoritative because it came from the controlled research tool for the same `work_id`. This repairs a display-field mismatch without accepting a different source or an uninspected work.

Candidate citations not inspected during the turn retain the existing demotion behavior: they are removed from structured candidate citations and may remain only as ordinary Markdown links where possible.

## Error Handling

Source URL mismatch, unknown inspected work, duplicate IDs, inflated evidence level, empty quotes, and oversized text remain hard validation failures. Only title mismatch stops being a fatal error.

The frontend continues rendering genuine backend failures in the conversation. No retry is added because the identified title mismatch can be resolved deterministically during validation.

## Tests and Verification

Frontend unit tests cover the pure bottom-distance calculation at, inside, and outside the 24px threshold. Component-level logic is structured so conversation replacement requests an immediate bottom position, pinned content growth follows, upward scrolling pauses, and returning to the bottom resumes.

Rust regression tests cover:

- A model-provided candidate title that differs from inspected evidence is replaced by the inspected title and accepted.
- Source URL mismatch still fails.
- Evidence-level inflation still fails.

Per project deployment policy, no tests, builds, formatters, or dependency installation run locally. The branch is pushed and all frontend and Rust verification runs in GitHub Actions. After CI succeeds, the change is merged, released from GitHub CI, and the matching `aarch64-unknown-linux-gnu` artifact is checksum-verified and deployed. Final acceptance requires the service health endpoint to report the new release version.
