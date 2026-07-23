---
title: Adapt launch and injection to ChatGPT unified desktop Change Review
module: launch-injection
change_id: 001-chatgpt-unified-desktop-compat
tags: [desktop-host, chatgpt, launch, cdp]
created: 2026-07-13
updated: 2026-07-13
status: active
summary: >
  Change proposal passes review with no blocking or recommended revisions before implementation.
---

# Adapt launch and injection to ChatGPT unified desktop Change Review

> Related Change: [change.md](change.md)
> Module Spec: [spec.md](../../spec.md)
> Review Date: 2026-07-13
> Change Status: draft

## Review Summary

The change is ready to enter implementation after approval. The Delta now correctly describes the move from the current standalone Codex launch baseline to a host-compatible model that includes the unified ChatGPT desktop app. The TODO list covers runtime launch logic, CDP target selection, diagnostics, UI/auto-launch wording, documentation, and manual verification. No P0 or P1 issues were found.

## P0 - Must Fix

None.

## P1 - Recommended Fix

None.

## P2 - Optional Optimization

### [P2-1] Keep command names stable for this change
- **Type**: Scope control
- **Description**: The implementation may be tempted to rename commands such as `launch_codex` or `reinject_codex`. Keeping command names stable in this change would reduce frontend/backend churn; user-facing labels can still change to desktop-host wording.

## Section 3 Compliance Check

| Delta Item | Type | Spec Location | Status | Notes |
|-----------|------|-----------|------|------|
| Unified ChatGPT host support | ADDED | 3.3 | OK | Current spec baseline excludes unified ChatGPT support, so this is a true addition. |
| Host kind diagnostics | ADDED | 3.2 | OK | Extends existing diagnostic output responsibility. |
| Renderer graceful degradation | ADDED | 3.3 | OK | Current baseline has renderer injection but not per-hook degradation. |
| Desktop host discovery | MODIFIED | 3.3 | OK | Current baseline discovers standalone Codex app paths only. |
| Process detection | MODIFIED | 3.3 | OK | Current baseline assumes `Codex` / `Codex.exe`. |
| CDP target selection | MODIFIED | 3.3 | OK | Current baseline prefers targets containing `codex`. |

## Section 3 To TODO Mapping Check

| Section 3 Item | Corresponding TODO | Coverage |
|--------|-----------|---------|
| Unified ChatGPT host support | TODO-S1, TODO-S2, TODO-C1, TODO-G1 | Complete |
| Host kind diagnostics | TODO-S4, TODO-G1 | Complete |
| Renderer graceful degradation | TODO-S4, TODO-G1 | Complete |
| Desktop host discovery | TODO-S1, TODO-S2 | Complete |
| Process detection | TODO-S1, TODO-S2 | Complete |
| CDP target selection | TODO-S3, TODO-S4 | Complete |

## AC Coverage Check

| Delta Item | Corresponding AC | Coverage |
|-----------|---------|---------|
| Unified ChatGPT host support | AC-2, AC-3, AC-4 | Covered |
| Host kind diagnostics | AC-3, AC-5 | Covered |
| Renderer graceful degradation | AC-5 | Covered |
| Desktop host discovery | AC-1, AC-2 | Covered |
| Process detection | AC-3 | Covered |
| CDP target selection | AC-2, AC-4 | Covered |

## Structure Completeness Check

| Section | Status | Notes |
|------|------|------|
| Section 1 Change Intent | OK | Motivation is clear and tied to the July 2026 desktop host shift. |
| Section 2 Scope | OK | In scope and out of scope are explicit. |
| Section 3 Spec Delta | OK | Delta is incremental against the current module baseline. |
| Section 4 Implementation Plan / TODO | OK | TODOs have descriptions, files, dependencies, and acceptance criteria. |
| Section 5 Acceptance Criteria | OK | Covers legacy, ChatGPT, restart, reinject, and degradation paths. |
| Section 6 Test Criteria | OK | Includes unit and manual scenario verification. |
| Section 7 Impact Assessment | OK | Compatibility, data, dependency, and rollback are covered. |
| Section 8 Risks And Mitigations | OK | Main packaging, debug, target, and renderer risks are covered. |
