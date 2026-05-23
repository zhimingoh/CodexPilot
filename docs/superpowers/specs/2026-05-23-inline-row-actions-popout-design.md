# Inline Row Actions Layout Design

## Context

CodexPilot injects inline session actions into the Codex sidebar so users can
export or delete a session without opening the floating Pilot panel.

The original implementation used two icon buttons placed inside the row with a
fixed absolute offset from the right side. That worked while Codex's native
trailing controls stayed narrow, but it became fragile once Codex showed more
native actions such as pinning. The Pilot buttons could then overlap or
visually crowd Codex-owned controls.

This is not a cosmetic issue only. Overlap makes ownership unclear and can
cause accidental clicks or blocked native actions.

## Goal

Keep inline session export/delete available, but stop them from covering Codex's
native sidebar controls.

## Non-Goals

- Do not remove inline actions entirely.
- Do not merge Pilot actions into Codex native controls.
- Do not redesign archive-row actions in this iteration beyond staying
  compatible with the existing inline-actions switch.
- Do not change export/delete backend behavior.

## Product Decision

Keep direct inline export/delete for normal session rows, but shift the action
cluster farther left into the row's blank area so it no longer covers Codex's
native trailing controls.

Behavior:

- the row still reveals Pilot actions on hover/focus;
- the visible affordance remains the same two direct action buttons:
  - export
  - delete
- there is no extra trigger and no secondary popout step.

This keeps one-step access while avoiding the overlap that made ownership and
click targets unclear.

## Placement And Direction Rules

The two buttons should live in the row's trailing blank area, not directly on
top of Codex-native controls. The reserved title mask should grow accordingly so
long titles fade out before colliding with the buttons.

## Interaction Model

- Hover or focus on a sidebar row reveals the two Pilot buttons.
- Export and delete keep their current click handling and safety behavior.

This means the interaction stays one step:

- reveal row actions
- click export or delete

## Existing Design Alignment

This change stays consistent with the existing enhancement specs:

- it remains governed by the `inlineActions` switch in
  `2026-05-21-page-enhancement-switches-design.md`;
- it does not alter HTML export behavior from
  `2026-05-21-html-export-design.md`;
- it does not affect Timeline, scroll restore, or the Pilot floating pill.

What changes is only the placement and collision-avoidance model for normal
session-row inline actions.

## Error Handling

- If the row cannot provide enough visual room, the buttons should still stay
  inside the row and fail soft rather than blocking Codex-owned controls.
- Any renderer errors must fail soft and not block Codex's native sidebar
  actions.

## Testing

Manual verification should cover:

- rows with Codex native trailing controls no longer show Pilot delete/export
  directly on top of them;
- hovering a row reveals direct export/delete buttons without an extra trigger;
- the title fade leaves readable space for the shifted button cluster;
- export and delete still invoke the same flows as before.

## Acceptance Criteria

- Normal session rows no longer place the direct two-button cluster on top of
  Codex native controls.
- Export and delete remain directly clickable from the row.
- The buttons are visually shifted into the row's blank space with matching
  title masking.
- Existing inline export/delete functionality remains available behind the new
  layout.
