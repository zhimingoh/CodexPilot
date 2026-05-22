# Manager Title Hierarchy Design

## Context

The current CodexPilot Manager uses three different header patterns in the same
visual layer:

- overview task cards use `taskHeader` plus `titleIcon` or `rowIcon`
- many panels use `panelHeader`
- some panels still place `panelTitle` directly above the body without a shared
  header container

This causes three visible problems:

1. title baselines and spacing do not align across pages
2. page names, card titles, and status badges visually compete with each other
3. labels such as `当前状态` and `检查项` are too generic for the task-oriented
   navigation introduced by the Manager redesign

The goal of this change is not to restyle everything. The goal is to make the
title system predictable, compact, and compatible with the existing vertical
task layout.

## Decision

Adopt a two-level title hierarchy:

```text
Page title -> explains the current workspace
Card title -> explains the task handled by this section
```

Status badges and action buttons are not part of the title. They belong in the
right side of the header row.

## Title Hierarchy

### Page Titles

Page titles remain the navigation-level labels:

- `启动与注入`
- `模型通道`
- `对话维护`
- `诊断`

They answer "where am I?" and "what kind of work happens on this page?".

Page titles should stay visually above card titles. They must not be recreated
inside the first card by mixing page identity with local task status.

### Card Titles

Card titles answer "what does this section do?".

Recommended card labels:

- Launch page:
  - `启动状态`
  - `运行环境`
  - `启动偏好`
  - `页面增强`
- Provider page:
  - rename `当前状态` to `通道状态`
  - keep `选择通道`
  - keep `配置档`
  - keep `官方通道`
- Session maintenance page:
  - keep `回收站`
  - keep `对话同步`
- Diagnostics page:
  - rename `检查项` to `运行检查`

`检查项` can still appear as a metric label on overview cards, but not as the
primary title of the diagnostics page content.

## Header Structure

All regular `panel` sections should use one shared structural pattern:

```tsx
<section className="panel">
  <div className="panelHeader">
    <div className="panelTitle">
      <Icon size={16} />
      <h2>Title</h2>
    </div>
    <div>{optional status / actions / path}</div>
  </div>
  ...
</section>
```

Rules:

- `panelHeader` owns the space between header and body
- `panelTitle` owns icon/title alignment, not bottom spacing
- panels without right-side content should still use `panelHeader`
- status pills should sit on the right side of the header, not immediately
  after the title text

## Overview Relationship

The overview page already uses a separate `taskPanel` and `taskHeader`
structure. That structure should stay for now because it encodes different card
priority and summary density.

This spec does not require migrating overview cards onto `panelHeader`.
Instead:

- keep overview task panels structurally separate
- align their title baseline and spacing closer to `panelHeader`
- continue using icon tiles only where they communicate task priority on the
  overview

This keeps the overview visually distinct without letting it drift into a
different title system.

## Visual Rules

- card title icon stays on the left of the title text
- right side of the header is reserved for status pills, code paths, and
  actions
- title text remains compact and operational rather than promotional
- page titles remain more prominent than card titles
- do not attach status wording directly to the title text

For this iteration, visual consistency matters more than adding a new reusable
component or redesigning every icon container.

## Implementation Scope

Keep the implementation intentionally small:

1. wrap remaining direct `panelTitle` usages in a `panelHeader`
2. move vertical spacing responsibility from `.panelTitle` to `.panelHeader`
3. align `taskHeader` title baseline with `panelHeader`
4. rename card titles:
   - `当前状态` -> `通道状态`
   - `检查项` -> `运行检查`

Do not:

- introduce a new shared React header component in this round
- restructure overview card content
- redesign icon tile colors or semantics beyond small alignment fixes

## Acceptance Criteria

- regular panels no longer mix direct `panelTitle` layout and `panelHeader`
  layout
- page-level labels and card-level labels no longer compete in the same visual
  role
- provider page uses `通道状态`
- diagnostics page uses `运行检查`
- title spacing is driven by the header container rather than by
  `.panelTitle { margin-bottom: ... }`
- overview remains structurally distinct but feels visually aligned with the
  rest of the Manager
