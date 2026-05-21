# Manager Vertical Task Layout Design

## Context

The current CodexPilot manager uses technical navigation groups: status, provider, recycle bin, diagnostics, and settings. This makes the app feel scattered:

- Status and settings both describe launch readiness but live on separate pages.
- Provider configuration mixes login detection, profile selection, editing, applying, clearing, and session sync.
- Recycle bin and diagnostics are maintenance tools but currently appear at the same level as launch and provider setup.
- The global launch button is always visible, even when the current page is not launch-focused.

The target layout should make the user's workflow clearer: first understand whether CodexPilot can run, then configure launch/injection, then configure provider routing, then maintain sessions and diagnostics.

## Decision

Use a manager layout based on:

```text
Overview workbench + vertical task sections
```

The left navigation remains, but the page model changes from technical modules to task-oriented sections.

## Navigation

Recommended navigation:

- Overview
- Launch & Injection
- Provider
- Session Maintenance
- Diagnostics

## Overview Page

The overview page should be a vertical task flow, not a two-column card grid.

Recommended order:

1. Launch readiness
2. Provider status
3. Session maintenance summary
4. Diagnostics summary

Each task section should have one clear purpose and one obvious next action.

## Layout Rule

Different tasks should stack vertically because they represent user priorities and next steps.

Information inside the same task may be arranged horizontally when it describes the same function. For example:

- Launch task: backend status, Codex path, debug port, helper port, launch button.
- Provider task: auth state, active provider, active profile, configured state.
- Session maintenance task: deleted count, selected count, restore/permanent delete actions.
- Diagnostics task: check state, collect/copy/export actions.

Do not arrange unrelated tasks as equal horizontal cards on the overview page.

## Page-Level Behavior

### Overview

Shows the current health and next action. It should answer:

- Can CodexPilot launch Codex now?
- Is CodexPilot connected to the helper backend?
- Is provider routing official or custom?
- Are there deleted sessions requiring attention?
- Are diagnostics healthy?

### Launch & Injection

Combines the current status and settings pages:

- Codex app path.
- Auto-detected path.
- Debug port.
- Helper port.
- Launch command preview.
- Save preferences.
- Launch action.

### Provider

This section has been superseded for detailed Provider behavior by
`2026-05-20-provider-channel-simplification-design.md`. Keep the task-cohesive
placement from this document, but follow the newer Provider spec for labels,
visible channel choices, in-place editing, and hidden official-channel API
fields.

Keeps profile selection and profile editing in the same task area:

- Top status strip: official login/auth file/provider summary.
- Mixed relay profiles are edited in place in the profile list.
- Official channel hides custom API fields.

### Session Maintenance

Contains recycle bin recovery and permanent deletion. This page can stay table-first.

### Diagnostics

Contains checks and logs. This page can stay utility-first.

## Visual Direction

- Keep the UI restrained and operational.
- Use fewer large panels on the overview page.
- Prefer compact task cards with clear headers, status rows, and action areas.
- Avoid nested cards.
- Avoid making maintenance tools visually equal to the primary launch/provider flow.

## Acceptance Criteria

- Launch status and launch settings are no longer split across two top-level pages.
- Overview uses vertical task sections.
- Only information within the same task is arranged horizontally.
- Provider remains task-cohesive, with mixed relay profile selection and editing visible together.
- Session maintenance and diagnostics are still reachable but visually secondary to launch and provider setup.
