# Page Enhancement Switches Design

## Context

CodexPilot already injects several visible helpers into Codex: the Pilot status pill, Timeline, inline session export/delete actions, archive actions, and thread scroll restore. These features are useful, but they all depend on Codex page structure. When Codex changes its DOM or a user wants to isolate a display issue, there is no single place to reduce or disable injected UI.

This design adds user-facing switches for existing page enhancements. It does not add a new enhancement feature.

## Goal

Add a `页面增强` section to the existing Launch view. Users can turn all injected page enhancements off, or keep the global switch on and control individual enhancement groups.

The switches should make troubleshooting safer without making CodexPilot feel like it has another top-level settings area.

## Non-Goals

- Do not add a new top-level Manager navigation item.
- Do not create a second launcher app or silent user-facing app.
- Do not add pure API mode UI.
- Do not implement plugin unlocking, forced plugin installation, remote scripts, or advertising content.
- Do not change Provider configuration, session storage, or Codex-owned files.

## User Experience

The Launch view gains a `页面增强` panel near the existing launch and injection status controls.

Initial switches:

- `页面增强总开关`: controls all visible renderer enhancements.
- `Timeline`: controls the right-side prompt timeline.
- `行内导出和删除`: controls session-list and archive-list inline export/delete actions, including Markdown and HTML export entries.
- `滚动恢复`: controls saving and restoring per-thread scroll position.

Default values are all on, preserving current behavior.

When the global switch is on, individual switches are editable. When the global switch is off, individual switches are disabled and visually muted, but they remain visible and keep their previous values. Re-enabling the global switch restores those individual values.

The disabled state should include a short note:

```text
页面增强已关闭，下面的分项设置会在重新打开后继续生效。
```

Switch changes are saved to CodexPilot-owned app state. They should not write to `~/.codex/config.toml`, session files, SQLite state, or Codex app installation files.

## Placement

The section belongs in the Launch view rather than a standalone menu because these controls are about injection behavior. Users debugging missing buttons, layout overlap, or "click does nothing" issues are most likely to look near launch/reinject status.

If local user scripts are added later, the product can revisit a top-level `增强` navigation item that contains both built-in enhancements and user scripts. That is out of scope for this change.

## Behavior

The Manager persists an enhancement settings object in CodexPilot app state:

```json
{
  "enabled": true,
  "timeline": true,
  "inlineActions": true,
  "scrollRestore": true
}
```

The renderer injection should read these settings before installing each feature group.

Expected feature gating:

- If `enabled` is false, do not render the Pilot pill, Timeline, inline actions, archive actions, or scroll restore handlers.
- If `enabled` is true and `timeline` is false, skip Timeline only.
- If `enabled` is true and `inlineActions` is false, skip session-list and archive-list inline actions. The Pilot pill can still exist.
- If `enabled` is true and `scrollRestore` is false, skip scroll position listeners and restoration.

The first implementation may apply changes on the next reinjection. The UI should make that clear with a concise status message after saving. Live hot-reload of already injected features is not required.

## Existing Design Alignment

This design narrows or disables behavior already covered by existing specs:

- Timeline remains governed by `2026-05-20-thread-timeline-design.md` when its switch is enabled.
- The Pilot status pill remains governed by `2026-05-20-subtle-pilot-pill-design.md` when global page enhancement is enabled.
- HTML export remains governed by `2026-05-21-html-export-design.md` when inline actions or Pilot export controls are enabled.

No existing design requires these features to be unconditionally visible. The new switches are therefore consistent with the existing designs.

## Error Handling

If settings cannot be read, CodexPilot should fail open and use defaults so existing users do not lose functionality.

If settings cannot be saved, the Manager should keep the old values and show a concise error. It should not partially update the UI as if the change succeeded.

If the renderer receives malformed settings, it should report a diagnostic event and use defaults. Diagnostics must not include API keys or full session content.

## Testing

Automated coverage should include:

- Manager settings serialization and default values.
- Global switch off disables and mutes individual switch controls while preserving their values.
- Renderer injection skips Timeline when `timeline` is false.
- Renderer injection skips inline actions when `inlineActions` is false.
- Renderer injection skips scroll restore when `scrollRestore` is false.
- Global `enabled: false` prevents all visible page enhancements from being installed.

Manual verification should check:

- The Launch view remains readable with the new panel.
- Turning the global switch off clearly shows affected subfeatures without hiding them.
- Re-enabling the global switch restores previous subfeature values.
- A reinjection applies the saved settings.
- Diagnostics, Provider configuration, and session maintenance remain available when page enhancements are disabled.

## Acceptance Criteria

- Page enhancement switches are available inside the Launch view.
- The global switch disables individual controls by muting them, not hiding them.
- Individual switch values are preserved while the global switch is off.
- Settings are stored only in CodexPilot-owned app state.
- Existing page enhancement behavior remains the default.
- Implementation stays consistent with the Timeline, Pilot pill, and HTML export specs.
