# Subtle Pilot Status Pill Design

## Context

CodexPilot currently injects a fixed bottom-right floating entry into Codex. The current entry is a prominent blue pill labeled "助手". This is robust because it does not depend on Codex sidebar or header DOM structure, but it is too visually loud for a control that stays on screen while the user is writing.

The design direction is to keep the compatible floating mechanism and reduce the default visual weight.

## Decision

Replace the current blue "助手" floating button with a compact status pill:

```text
● Pilot
```

The pill remains fixed near the bottom-right corner and continues to open the existing CodexPilot panel when clicked.

## Visual Behavior

- Default state uses only the compact pill, with no large outer white container.
- Pill background is near-white and lightly translucent.
- Border is subtle and neutral.
- Shadow is light enough to separate the pill from Codex content without reading as a primary action.
- Text label is `Pilot`.
- The status dot appears before the label.
- Hover slightly strengthens the background, border, and shadow.
- Open state may use the same pill or a slightly stronger active border, but should not switch to a blue filled button.

## Status Semantics

- Green dot: CodexPilot bridge/backend is connected and healthy.
- Yellow dot: connection check is in progress.
- Gray dot: not connected, not checked, or unavailable.

Detailed status text belongs inside the opened panel, not in the always-visible pill.

CodexPilot checks backend status automatically when the injected menu is created, then refreshes it periodically. Users should not need to click a status check action just to make the dot trustworthy.

If the backend status heartbeat times out repeatedly after a long lock screen or
sleep/wake cycle, CodexPilot should treat that as a stale bridge signal rather
than immediately assuming the backend process is gone. After three consecutive
status timeouts, the renderer should request a bridge recovery through the
existing bridge channel. Recovery is rate-limited and reinjects the current
Codex page; it does not restart Codex or CodexPilot.

## Panel Behavior

The existing panel structure should remain small:

- Header: `CodexPilot` and version.
- Export current session action.
- Short backend and action message area.

High-risk session actions such as delete should remain near the session rows rather than becoming prominent global actions in the floating panel.

## Compatibility Rationale

Keeping the bottom-right injected entry avoids depending on Codex's sidebar grouping, topbar layout, or internal navigation DOM. This should remain more resilient to Codex updates than integrating CodexPilot into the native sidebar or header.

## Implementation Notes

- Preserve the existing `#codex-pilot-root` and `createMenu` injection flow.
- Rename visible text from `助手` to `Pilot`.
- Add a status dot element inside the toggle button.
- Track status with a root/button data attribute such as `data-status="unknown|checking|connected"`.
- On menu creation, default to `checking`.
- Automatically call the existing backend status bridge on creation and on a short heartbeat.
- When checking backend status, set `checking`, then `connected` on success or `unknown` on failure.
- Track consecutive backend status timeouts. After three consecutive timeouts,
  report a diagnostic event and call `/backend/recover-bridge`.
- Rate-limit recovery attempts so a stale page cannot continuously reinject.
- Implement `/backend/recover-bridge` by reusing the existing current-page
  injection path with the active debug and helper ports.
- Use CSS custom properties or status selectors to map dot color.
- Keep row hover actions out of this design unless a follow-up explicitly targets them.

## Acceptance Criteria

- The injected entry is less visually prominent than the current blue button.
- The entry still clearly communicates that CodexPilot is available.
- The entry shows connection state through dot color.
- Backend status refreshes automatically without a manual check button.
- After repeated heartbeat timeouts, CodexPilot records diagnostics and attempts
  to recover by reinjecting the bridge once per cooldown window.
- Clicking the entry opens the same panel as before.
- No new dependency on Codex sidebar/header DOM is introduced.
