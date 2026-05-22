# Single App Auto Launch Design

## Status

Active with one constraint: automatic launch is an opt-in manager preference,
not the default startup behavior. CodexPilot remains one visible app and
`codex-pilot-launcher` remains an internal sidecar. The manager may trigger the
same launch command used by the manual button after startup, but only when no
Codex window is already running.

## Goal

CodexPilot should remain one visible product. Users should not see a second
CodexPilot launcher app or have to choose between two similar entries. The
daily path should be: open CodexPilot, let it start and inject Codex
automatically, and only show management UI when the user needs configuration or
when startup fails.

## Non-Goals

- Do not add a second `.app`, desktop shortcut, or Start menu product entry.
- Do not rename the existing product.
- Do not replace the existing manual "launch Codex" workflow.
- Do not hide failures silently.

## User Experience

The manager remains the explicit entry point for launch and configuration. The
Launch page includes an "auto launch on open" switch, saved with the same launch
preferences as app path and ports.

When the switch is off, opening CodexPilot only refreshes status. Users launch,
reinject, restart, and save preferences from the manager UI.

When the switch is on, opening CodexPilot silently starts Codex only when Codex
is not already running. If Codex is already open, even with a reachable debug
port, the manager skips automatic injection and opens the management UI. Users
can then choose the explicit manual reinject action. Startup failures remain
visible through the normal message and launch status surfaces.

## Architecture

Keep the existing `codex-pilot-launcher` binary as an internal sidecar. It
continues to own provider sync, Codex process startup, helper startup, and page
injection.

The Tauri manager owns the product entry point and launch preferences. On app
startup, it loads preferences and exposes the current launch snapshot. The
snapshot includes the auto-launch preference so the frontend can decide whether
to trigger the existing backend launch command once.

The frontend should not duplicate launch logic. It should call a backend command
for explicit launch/reinject actions and for safe automatic launch. It must not
call reinject or restart automatically when a Codex window is already running.

## Startup Flow

1. Manager starts normally.
2. Backend loads launch preferences for app path and ports.
3. Frontend receives `launch_snapshot`.
4. If `autoLaunchOnOpen` is off, no automatic launch is attempted.
5. If `autoLaunchOnOpen` is on, the frontend triggers at most one automatic
   action per manager page load:
   - helper already running: mark as running and do not spawn another launcher.
   - debug port reachable: skip automatic injection and keep the manager UI
     ready for a manual reinject.
   - no Codex running: spawn the sidecar launcher.
   - unrelated Codex already running: surface the current "restart required"
     state instead of killing it automatically.
6. Manual launch keeps handling all cases, including the confirmed restart path.
7. On failure, the manager stays visible and shows the error.

## Error Handling

Launch must be conservative. It must not close or restart an existing Codex
process without explicit user confirmation.

Errors should be written to the existing diagnostic log. The launch view should
show the latest failure message in the same style as manual launch failures.

If the app path is missing or invalid, auto launch should not loop. It should
show the manager and let the user fix the path.

If Codex is already running without the configured debug port, auto launch must
not call restart. It should show the manager and keep the existing confirmation
flow on the manual button.

## Packaging

Packaging remains single-product:

- macOS DMG contains only `CodexPilot.app`.
- Windows installer keeps one product entry for CodexPilot.
- `codex-pilot-launcher` remains bundled as an internal sidecar only.

No user-facing launcher app, shortcut, or second product name is added.

## Testing

- Unit-test launch preference serialization.
- Verify opening the manager does not launch Codex automatically when the switch
  is off.
- Verify opening the manager launches Codex once when the switch is on and no
  Codex window is running.
- Verify opening the manager does not automatically reinject when Codex is
  already running with a reachable debug port.
- Verify opening the manager does not restart an unrelated running Codex.
- Keep the frontend auto-launch decision in a small unit-tested module so these
  branches can be checked without spawning Codex.
- Verify the existing manual launch button still works.
- Verify failure states keep the manager visible and write diagnostics.
- Run existing Rust tests and renderer injection tests.
