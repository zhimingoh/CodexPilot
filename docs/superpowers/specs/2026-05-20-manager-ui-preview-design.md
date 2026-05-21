# Manager UI Preview Design

## Context

The CodexPilot manager UI is a Tauri React app. UI iteration currently depends on
running the full Tauri development flow, because `main.tsx` calls Tauri commands
directly through `invoke`. That makes visual changes slower to inspect and makes
plain Vite browser preview fail when Tauri APIs are unavailable.

The desired workflow is a development-only browser preview that shows realistic
manager data without launching the desktop shell or helper backend.

## Decision

Add a dedicated development script:

```text
npm run preview:ui
```

The script runs Vite in a `ui-preview` mode. In that mode, the frontend uses
mock command responses instead of Tauri `invoke`. Normal Tauri development and
production builds keep using real Tauri commands.

This is a developer tool, not an end-user feature. It must not add visible
product navigation, settings, or runtime UI inside the manager app.

## Scope

In scope:

- Add a thin frontend backend-call adapter.
- Move direct manager UI command calls from `invoke` to the adapter.
- Add mock snapshots for the manager pages.
- Add the `preview:ui` npm script.
- Verify TypeScript and browser preview behavior.

Out of scope:

- Static HTML export.
- Storybook or a component gallery.
- New product UI for switching mock mode.
- Changes to backend command behavior.

## Architecture

Create a frontend adapter, for example `src/backend.ts`, with an API shaped like:

```ts
callBackend<T>(command: string, args?: unknown): Promise<T>
```

The adapter chooses between:

- real Tauri `invoke` when `import.meta.env.MODE !== "ui-preview"`;
- local mock responses when `import.meta.env.MODE === "ui-preview"`.

Manager components should continue to think in command names and payloads. They
should not contain preview-mode conditionals.

## Mock Data

Create preview data under `src/dev/mockSnapshots.ts`.

Mock responses must cover all commands used by the current manager UI:

- `backend_status`
- `launch_snapshot`
- `provider_snapshot`
- `recycle_bin_snapshot`
- `diagnostics_snapshot`
- `app_version`
- `launch_codex`
- `reinject_codex`
- `restart_codex_and_inject`
- `save_launch_preferences`
- `save_provider_profile`
- `apply_provider`
- `activate_provider_profile`
- `delete_provider_profile`
- `clear_provider`
- `restore_recycle_bin_entries`
- `delete_recycle_bin_entries`
- `collect_diagnostics`

The mock snapshots should exercise real layout pressure:

- launch state is ready and includes ports, app path, command preview, and detail
  text;
- provider state is authenticated and includes at least two hybrid API profiles;
- recycle bin includes recoverable and non-recoverable entries;
- diagnostics include both healthy and attention-needed checks;
- action commands return success messages so buttons remain usable in preview.

The mock layer does not need persistent mutation. After a mock action, the normal
refresh can reload the same snapshots.

## Package Script

Add this script to `apps/codex-pilot-manager/package.json`:

```json
"preview:ui": "vite --host 127.0.0.1 --port 1420 --mode ui-preview"
```

Keep existing scripts unchanged:

- `dev` remains the full Tauri development command;
- `vite:dev` remains a raw Vite development command;
- `build` and `vite:build` continue to use real production behavior.

## Error Handling

If preview mode receives an unknown command, the adapter should reject with a
clear error message naming that command. This makes missing mock coverage obvious
during UI work.

Real Tauri mode keeps existing error behavior from `invoke`.

## Verification

Implementation is accepted when:

- `npm run check` passes in `apps/codex-pilot-manager`;
- `npm run preview:ui` starts Vite on `http://127.0.0.1:1420`;
- opening the preview URL renders the manager UI without Tauri API errors;
- overview, launch, provider, recycle bin, and diagnostics pages can be opened;
- action buttons in preview show success or progress messages instead of failing
  because Tauri is unavailable.
