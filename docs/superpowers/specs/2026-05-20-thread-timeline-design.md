# Thread Timeline Design

## Context

CodexPilot already injects a small floating Pilot control into Codex and adds session-level actions such as Markdown export and delete. Long Codex conversations are harder to navigate because the user has to scroll through all assistant output to find earlier prompts.

The injected renderer script already contains an early Timeline implementation. This design turns that existing direction into an explicit, supported feature and keeps the scope narrow.

## Goal

Add a lightweight Timeline to Codex conversation pages. The Timeline gives users quick jump points for their own prompts in the current thread.

## Non-Goals

- Do not implement plugin entry unlocking or forced plugin installation.
- Do not add a manager settings page or persisted Timeline configuration in this iteration.
- Do not mark assistant messages, tool calls, errors, or internal events.
- Do not write to Codex session data.

## User Experience

When the current page is a Codex conversation with at least two detected user turns, CodexPilot shows a narrow vertical Timeline near the right edge of the page.

Each marker represents one user prompt:

- Hover or focus shows a short text preview of the prompt.
- Click scrolls the corresponding prompt into the center of the viewport.
- The marker label is accessible to screen readers, for example `跳转到第 3 个问题`.

The Timeline is hidden when:

- the current page is not a conversation;
- the current session cannot be detected;
- fewer than two user prompts are found;
- the message DOM cannot be read safely.

## Detection Strategy

The Timeline should detect user prompt nodes from the live Codex page DOM. The first implementation should keep a selector fallback list, ordered from most semantic to least specific:

- `[data-message-author-role='user']`
- `[data-testid*='conversation-turn']`
- `[data-testid*='user-message']`
- `[class*='user-message']`

For each candidate, CodexPilot extracts normalized text, removes CodexPilot action labels, requires at least two visible characters, and ignores nodes with zero-height boxes when layout information is available.

The implementation should cap candidates to a reasonable limit, currently 80, so large threads do not produce excessive DOM work or unreadable marker density.

## Rendering

The Timeline is a fixed-position injected element owned by CodexPilot:

- root id: `codex-pilot-timeline`;
- right edge placement, separate from the Pilot floating control;
- a thin neutral track;
- small circular markers with pointer events enabled;
- tooltip positioned to the left of the marker.

Marker vertical positions are calculated from each prompt node's approximate scroll offset divided by the current thread scroll height. The position should be clamped between 2% and 98% to keep markers reachable.

Rendering must be idempotent. If the current session, count, and prompt signature have not changed, CodexPilot should keep the existing Timeline instead of rebuilding it.

## Refresh Behavior

CodexPilot refreshes the Timeline when:

- the injected script starts;
- the page mutates;
- the active thread changes through history navigation or sidebar selection;
- a periodic safety interval runs.

The refresh path must be fail-soft. Any exception removes the Timeline and reports a diagnostic event, without breaking the Codex page.

## Diagnostics

Timeline diagnostics should use the existing renderer diagnostic bridge:

- `timeline_rendered` with session id and marker count;
- `timeline_jump` with session id, marker index, and a short prompt preview;
- `timeline_no_targets` throttled so empty pages do not spam logs;
- `timeline_error` with a short error message.

Diagnostics should not include full prompt contents. Previews should be truncated.

## Data And Security

Timeline is read-only. It reads the current page DOM and does not modify Codex session files, state databases, auth data, model provider config, or CodexPilot profile data.

The feature injects DOM elements and event handlers into the Codex renderer. It must only run inside the existing CodexPilot injection flow and should preserve the existing local-trust security model.

## Testing

Automated coverage should extend the existing renderer injection test with representative conversation DOM fixtures:

- no conversation page hides Timeline;
- one user prompt hides Timeline;
- multiple user prompts render markers;
- clicking a marker calls `scrollIntoView` on the matching prompt;
- re-rendering with the same prompt signature does not duplicate the root.

Manual verification should check a real or mocked Codex page after injection:

- Timeline appears on long threads;
- tooltip text is readable and does not overlap the Pilot panel;
- marker clicks scroll to the expected prompts;
- switching threads refreshes or removes the Timeline correctly.

## Acceptance Criteria

- Timeline is documented as a supported CodexPilot feature.
- Timeline renders only when at least two user prompts are detected in the active thread.
- Markers are stable, clickable, and show prompt previews.
- Timeline failures do not affect export, delete, scroll restore, or the Codex page itself.
- Plugin unlocking remains out of scope for this change.
