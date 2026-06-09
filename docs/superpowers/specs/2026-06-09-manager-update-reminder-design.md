# Manager Update Reminder Design

## Goal

CodexPilot Manager should tell users when a newer published version is available without taking over installation or adding a full auto-updater in the first version.

The reminder should feel similar to the lightweight `sub2api` version card: users can see the current version, manually re-check, open the release page, and ignore a specific version.

## Scope

The first version includes:

- a Manager header update entry with a quiet default state;
- manual and startup update checks against the latest GitHub Release;
- a compact popover showing current version, latest version, status, and actions;
- local dismissal for one latest version;
- diagnostics-friendly backend errors without noisy runtime logging.

The first version does not include:

- automatic download or installation;
- asset selection for macOS or Windows installers;
- background replacement of the running app;
- update channels such as beta, nightly, or prerelease.

## Existing Context

CodexPilot already publishes GitHub Releases and has a release-notes design in `docs/superpowers/specs/2026-05-21-release-notes-design.md`. The Manager reads the current app version through the `app_version` command and shows it on the overview page.

This design builds on that release flow instead of inventing a separate update feed.

## Recommended Approach

Use GitHub's latest release API:

```text
https://api.github.com/repos/hl9565/CodexPilot/releases/latest
```

The backend compares the release tag, such as `v1.3.3`, with `codex_pilot_core::version::VERSION`, such as `1.3.2`. A newer stable release becomes an update reminder. Equal or older versions render as "already latest".

The UI opens the release page instead of downloading an installer. That keeps the first version small, platform-neutral, and aligned with the existing release asset workflow.

## Backend Design

Add a focused update module under the Manager Tauri command layer.

New command:

```rust
#[tauri::command]
pub(crate) async fn check_latest_release(app: tauri::AppHandle) -> Result<UpdateSnapshot, String>
```

The command must:

- be `async`;
- use `tauri::async_runtime::spawn_blocking` for local preference reads/writes when needed;
- use `codex_pilot_core::http_client::shared()` before adding any new `reqwest::Client`;
- not hold a `std::sync::Mutex` guard across `.await`;
- emit a Tauri event after the update state changes.

Suggested event:

```text
update_state_changed
```

Suggested response shape:

```ts
type UpdateSnapshot = {
  currentVersion: string;
  latestVersion: string | null;
  latestTag: string | null;
  releaseUrl: string | null;
  releaseName: string | null;
  publishedAt: string | null;
  status: "checking" | "latest" | "available" | "ignored" | "failed";
  error: string | null;
};
```

The backend should ignore prereleases and drafts by relying on GitHub's `/releases/latest` endpoint. If GitHub returns no suitable release, the command returns `failed` with a concise error.

Version comparison should normalize a leading `v` and compare semantic version numbers. Suffix versions such as `1.3.3-beta.1` should not be considered newer than a stable installed version in the first version.

## Dismissal Design

Add one local preference for ignored release tags:

```json
{
  "ignoredUpdateTag": "v1.3.3"
}
```

This should live in the existing Manager configuration path or a small focused Manager update settings file resolved through existing path helpers. Do not scatter new platform-specific path logic.

New command:

```rust
#[tauri::command]
pub(crate) async fn ignore_latest_release(tag: String) -> Result<UpdateSnapshot, String>
```

When the latest tag matches `ignoredUpdateTag`, the snapshot status is `ignored`. The header should stop showing the attention marker, but the popover should still show the ignored version and allow a manual re-check.

## Frontend Design

Place a small icon button in the Manager header next to the refresh and theme buttons.

States:

- `unknown`: neutral icon before the first check resolves;
- `checking`: subtle spinner or disabled refresh-in-popover state;
- `latest`: neutral icon, popover says the current version is latest;
- `available`: icon shows a small attention dot;
- `ignored`: neutral icon, popover says this version was ignored;
- `failed`: neutral icon, popover shows the failure only when opened.

Popover content:

```text
当前版本
v1.3.2

发现新版本 v1.3.3

重新检查    查看发布    忽略此版本
```

For `latest`:

```text
当前版本
v1.3.2

已是最新版本

重新检查    查看发布
```

`查看发布` opens the GitHub Release URL when available. If no release URL is known, the button is disabled.

The startup check should run once after the Manager loads. Manual `重新检查` should always call the backend command again.

## Error Handling

Network failure, GitHub API failure, JSON parsing failure, or invalid tag formats should not interrupt launch or injection workflows.

The header icon remains quiet on failure. The popover shows a short message such as:

```text
暂时无法检查更新
```

Detailed failure context can be returned in the command result and logged through `codex_pilot_core::diagnostic_log` if persistent diagnostics are needed. Do not add runtime `println!`.

## IPC and Events

The frontend should update from command results and the `update_state_changed` event. It should not poll GitHub or poll the backend to detect update state changes.

Startup behavior:

1. Manager loads current app version.
2. Manager calls `check_latest_release`.
3. Backend emits `update_state_changed`.
4. Frontend updates the header icon and popover state.

Manual check behavior:

1. User clicks `重新检查`.
2. Frontend shows checking state.
3. Backend fetches the latest release.
4. Backend emits `update_state_changed`.
5. Frontend replaces the local snapshot.

## Platform Notes

This feature touches network I/O and local config paths, but not subprocesses or visible window lifecycle.

Windows considerations:

- update settings path resolution must reuse existing path helpers;
- the release page open action must use the existing Tauri shell/open pattern if one exists, or add a focused cross-platform helper;
- no installer asset is downloaded or executed in this version.

## Testing

Add tests for:

- version normalization and semantic comparison;
- equal, older, newer, invalid, and suffix version tags;
- ignored latest tag state;
- frontend rendering for latest, available, ignored, and failed snapshots.

Manual verification:

- startup check shows no blocking dialog;
- available update shows only a small header marker;
- popover text fits in the current Manager header layout;
- `查看发布` opens the release URL;
- `忽略此版本` clears the attention marker for the same tag;
- failure state does not affect launch, reinject, refresh, diagnostics, or session tools.

## Design Consistency

This design extends the existing release-notes flow and Manager header behavior. It does not change release publishing, package asset generation, launch behavior, diagnostics behavior, or session tooling.

Implementation should update this spec if it chooses a manifest feed or full Tauri updater instead of the GitHub Release reminder described here.
