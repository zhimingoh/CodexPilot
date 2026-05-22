# CCSwitch Config Import Design

## Context

CodexPilot already has a local provider/profile model for `模型通道`, but it
does not yet offer a way to bring existing third-party relay settings into that
model. Users who already maintain Codex providers in CCSwitch currently need to
retype the same Base URL, API Key, and protocol choice by hand.

Before designing this change, we checked how CodexPlusPlus implements CCS
import:

- the Manager UI shows a small CCS import row inside the relay profile area;
- the Tauri command reads `~/.cc-switch/cc-switch.db`;
- it selects `providers` rows where `app_type = 'codex'`;
- it extracts a small normalized provider shape from `settings_config`;
- it appends new local relay profiles and skips duplicates.

Relevant references:

- `../CodexPlusPlus/crates/codex-plus-core/src/ccs_import.rs`
- `../CodexPlusPlus/apps/codex-plus-manager/src-tauri/src/commands.rs`
- `../CodexPlusPlus/apps/codex-plus-manager/src/App.tsx`

CodexPilot should follow the same spirit, but not copy the full storage model.
CodexPilot profiles only persist:

- `id`
- `name`
- `baseUrl`
- `bearerToken`
- `mode`
- `upstreamProtocol`

They do not store raw CCS `config` text or `auth` text. The first version
should therefore be a narrow field-mapping import, not a full CCS config mirror.

## Goals

- Let users import existing CCSwitch Codex provider settings into CodexPilot.
- Reuse CodexPilot's current provider/profile storage and apply flow.
- Keep the first version low-risk: import should add local profiles, not change
  the active channel automatically.
- Preserve task cohesion on the `模型通道` page instead of creating a separate
  advanced import flow.

## Non-Goals

- Do not implement a full CCS config editor inside CodexPilot.
- Do not preserve CCS raw `config` or `auth` contents in CodexPilot storage.
- Do not auto-apply an imported profile.
- Do not auto-switch the current channel after import.
- Do not add a multi-step wizard, preview table, or per-row checkbox flow in v1.
- Do not support custom CCS database paths in v1.
- Do not import non-`codex` CCS providers.
- Do not trigger Provider Sync as part of import.

## Product Decision

CCSwitch import is a secondary profile-source action inside `模型通道`, not a
separate maintenance tool.

Reasoning:

- importing CCS data produces new local `配置档`;
- users will look for that action where they already create, select, and edit
  provider profiles;
- it does not belong under diagnostics or session maintenance because it does
  not inspect health and does not rewrite historical session ownership.

## Entry Placement

Place the import entry in the `模型通道` page, inside the `配置档` panel, above
the existing profile list.

Recommended UI shape:

- label: `CCSwitch 配置`
- summary text: `已发现 N 个可导入配置` or a readable empty/error state
- secondary action: `刷新`
- secondary action: `导入`

This should be a compact single row, visually lighter than the main profile
editor. It is a source/import affordance, not the primary action of the page.

## External Data Read

The backend should read the default CCS database path:

- `~/.cc-switch/cc-switch.db`

It should query:

- table: `providers`
- filter: `app_type = 'codex'`
- order: consistent with CCS row order, matching the existing CodexPlusPlus
  behavior when practical

For each row, the import pipeline should derive a normalized candidate record
with only these fields:

- `sourceId`
- `name`
- `baseUrl`
- `apiKey`
- `upstreamProtocol`

### Field Extraction

Follow the CodexPlusPlus extraction behavior closely:

- `baseUrl`
  - prefer top-level `base_url` or `baseURL`
  - then `config.base_url` or `config.baseURL`
  - then parse `base_url` out of string TOML `config`
- `apiKey`
  - prefer `env.OPENAI_API_KEY`
  - then `auth.OPENAI_API_KEY`
  - then top-level `apiKey` or `api_key`
  - then `config.apiKey` or `config.api_key`
- `upstreamProtocol`
  - if `api_format`/`apiFormat` is a chat variant, map to `chatCompletions`
  - else if TOML `config` contains `wire_api = "chat"` or equivalent, map to
    `chatCompletions`
  - else if `baseUrl` ends with `/chat/completions`, map to
    `chatCompletions`
  - otherwise map to `responses`

Rows that cannot produce a non-empty `baseUrl` should be skipped.

## CodexPilot Mapping

Each imported CCS candidate becomes one new CodexPilot local provider profile.

Mapping:

- `candidate.name -> profile.name`
- `candidate.baseUrl -> profile.baseUrl`
- `candidate.apiKey -> profile.bearerToken`
- `candidate.upstreamProtocol -> profile.upstreamProtocol`
- `profile.mode = api`

### Why `api` Instead of `hybridApi`

This is intentional.

CCSwitch provider data expresses an API-backed provider. It does not express
that the user wants to preserve Codex/ChatGPT official login while relaying
requests. Importing into `hybridApi` by default would silently add product
meaning that does not exist in the source data.

If a user wants an imported profile to become `混合中转`, they can switch that
profile inside CodexPilot after import. The import itself should stay faithful
to the source intent.

## Conflict Handling

Import should never overwrite an existing CodexPilot profile in v1.

Conflict policy:

1. If an existing profile already matches all of these normalized fields, skip
   the CCS row as already imported:
   - trimmed lowercase `name`
   - normalized `baseUrl` without trailing slash, lowercase
   - `mode`
   - `upstreamProtocol`
2. Otherwise, if the visible profile name conflicts with an existing profile
   name after trimming and case folding, create a new unique name:
   - `Name (CCS)`
   - `Name (CCS 2)`
   - `Name (CCS 3)`
3. Generate a fresh local profile `id`; do not reuse CCS ids as persisted
   CodexPilot ids.

This differs slightly from the simpler CodexPlusPlus duplicate rule. That is
correct because CodexPilot's stored profile identity includes `mode` and
`upstreamProtocol` as first-class user-facing attributes, so duplicate handling
should align with the CodexPilot model rather than blindly copy another app's
shortcut.

## Apply and Activation Behavior

Import is an add-only local configuration action.

After import:

- imported profiles appear in the local `配置档` list;
- the current active profile remains unchanged;
- the current selected channel remains unchanged;
- no call to `apply_provider` is triggered automatically;
- no call to `clear_provider` is triggered automatically.

This keeps import low-risk and easy to understand. A user who clicks `导入`
expects to gain reusable profiles, not to silently change the running route for
future Codex requests.

## Backend Shape

The first version should add two Tauri commands:

1. `ccs_provider_snapshot`
   Returns:
   - db path
   - discovered candidate count
   - optional lightweight candidate list or normalized summary
   - last read status / error message suitable for UI display

2. `import_ccs_provider_profiles`
   Performs the import into CodexPilot provider-profile storage and returns:
   - imported count
   - skipped count
   - renamed count
   - updated provider snapshot after import
   - CCS read status if relevant

The import command should reuse the same provider-profile persistence file that
`save_provider_profile` already uses. It should not create a second storage
format.

## UI Behavior

On page load and on manual refresh, the Manager reads CCS snapshot data and
shows one of these states:

- database not found
- read failed
- no Codex CCS providers found
- found `N` importable CCS providers

The `导入` button should be disabled when there are zero importable candidates.

On successful import, the page should:

- show a result message like `已导入 3 个 CCSwitch 配置，跳过 1 个，重命名 1 个。`
- refresh the local provider snapshot
- refresh the CCS snapshot

## Error Handling

- Missing CCS database is not a hard error. Treat it as an empty source with a
  readable message.
- Unparseable CCS rows are skipped individually rather than failing the whole
  import.
- If local profile save fails, report the backend error and leave the current
  CodexPilot provider state unchanged.
- If import partially succeeds before a local save failure, the final persisted
  result should still be atomic from the user's perspective. Prefer building the
  full next profile state in memory first, then writing once.

## Existing Design Alignment

This design extends the current provider/page specs without changing their core
structure.

Still true:

- `模型通道` remains the right home for provider profiles.
- Provider Sync remains separate maintenance and does not run automatically.
- The primary user action of the channel page is still channel/profile save, not
  import.

What changes:

- the `配置档` area now supports an external source for creating local profiles,
  in addition to manual creation.

This means implementation should update the relevant provider UI/design docs if
their wording implies that the only way to create a profile is the inline `+`
button.

## Acceptance Criteria

- The `模型通道` page shows a compact `CCSwitch 配置` import row inside the
  `配置档` area.
- CodexPilot reads CCS data from `~/.cc-switch/cc-switch.db`.
- Only CCS rows with `app_type = 'codex'` are considered.
- Import maps CCS data into CodexPilot local provider profiles using the
  existing profile storage format.
- Imported profiles default to `mode = api`.
- Import does not automatically apply or activate a profile.
- Existing equivalent profiles are skipped.
- Visible profile-name conflicts are resolved by generating unique local names
  instead of overwriting existing profiles.
- Import result feedback reports imported/skipped/renamed counts.
- Missing CCS database and malformed individual rows do not crash the page.

