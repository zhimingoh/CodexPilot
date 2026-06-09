# Manager Update Reminder Design

## Goal

CodexPilot Manager should tell users when a newer published version is available without taking over installation or adding a full auto-updater in the first version.

The reminder should feel similar to the lightweight `sub2api` version card: users can see the current version, manually re-check, open the release page, and ignore a specific version. Unlike `sub2api`, users should not have to configure a GitHub proxy for the common path.

## Scope

The first version includes:

- a Manager header update entry with a quiet default state;
- manual and startup update checks against a small static update feed, with GitHub Release API as fallback;
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

Use a static update feed as the primary source:

```text
docs/update/latest.json
```

The published feed contains only release metadata:

```json
{
  "latestTag": "v1.3.3",
  "latestVersion": "1.3.3",
  "releaseUrl": "https://github.com/hl9565/CodexPilot/releases/tag/v1.3.3",
  "releaseName": "CodexPilot v1.3.3",
  "publishedAt": "2026-06-09T00:00:00Z"
}
```

The backend compares the feed tag, such as `v1.3.3`, with `codex_pilot_core::version::VERSION`, such as `1.3.2`. A newer stable release becomes an update reminder. Equal or older versions render as "already latest".

The UI opens the release page instead of downloading an installer. That keeps the first version small, platform-neutral, and aligned with the existing release asset workflow.

The feed should be available through multiple read-only URLs:

1. Gitee raw URL, backed by a Gitee mirror of the GitHub repository;
2. jsDelivr's GitHub CDN URL;
3. GitHub's `/releases/latest` API as the final fallback.

This avoids relying on public GitHub proxy services. Free proxy domains may disappear, rate-limit, or alter responses, so they should not be built into the product's default update path.

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
- try the static feed URLs before the GitHub Release API;
- prefer the highest valid version across static feed URLs;
- use GitHub Release API only when every static feed URL fails;
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

The backend should ignore prereleases and drafts by updating the feed only when a non-prerelease GitHub Release is published. The GitHub API fallback uses `/releases/latest`, which also excludes drafts and prereleases. If no source returns a valid release, the command returns `failed` with a concise error.

Version comparison should normalize a leading `v` and compare semantic version numbers. Suffix versions such as `1.3.3-beta.1` should not be considered newer than a stable installed version in the first version. Static feeds whose `latestVersion` disagrees with `latestTag` should be rejected.

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

Static feed failures should be quiet. When a feed mirror is stale or unavailable, the backend continues to the next static source. A valid static feed is trusted even if it is temporarily behind GitHub, because the common path should avoid waiting on a direct GitHub API call in constrained networks.

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

This feature touches network I/O, local config paths, release workflow files, and the release-page subprocess used by `查看发布`. It does not change visible window lifecycle.

Windows considerations:

- update settings path resolution must reuse existing path helpers;
- the release page open action must use the existing Tauri shell/open pattern if one exists, or add a focused cross-platform helper;
- no installer asset is downloaded or executed in this version.

## Release Feed Publishing

Add `docs/update/latest.json` to the repository and update it from a GitHub Actions workflow when a stable release is published. The workflow writes the release tag, normalized version, release URL, release name, and published timestamp, then commits the file back to `main`.

Gitee should be configured separately as a mirror of the GitHub repository. Its automatic pull mirror can sync this static JSON file without requiring the Manager user to configure a proxy. The Gitee mirror is a distribution source, not the authoritative source of release truth.

## Testing

Add tests for:

- version normalization and semantic comparison;
- equal, older, newer, invalid, and suffix version tags;
- static feed parsing, tag/version consistency, and multi-source highest-version selection;
- ignored latest tag state;
- frontend rendering for latest, available, ignored, and failed snapshots.

Manual verification:

- startup check shows no blocking dialog;
- available update shows only a small header marker;
- popover text fits in the current Manager header layout;
- `查看发布` opens the release URL;
- `忽略此版本` clears the attention marker for the same tag;
- failure state does not affect launch, reinject, refresh, diagnostics, or session tools.
- one stale or unavailable static feed does not hide a newer valid static feed from another source.

## Design Consistency

This design extends the existing release-notes flow and Manager header behavior. It does not change release publishing, package asset generation, launch behavior, diagnostics behavior, or session tooling.

Implementation should update this spec if it chooses a full Tauri updater or installer download path instead of the static feed reminder described here.
