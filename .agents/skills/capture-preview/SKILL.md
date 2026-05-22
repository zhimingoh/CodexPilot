---
name: capture-preview
description: "Use for CodexPilot screenshot requests that should open a local preview in the Codex in-app browser, capture clean images, and save them to a requested location."
---

# Capture Preview

Use this skill when the user asks for screenshots of CodexPilot pages, preview
surfaces, or local web UI states in this repository.

The point of this skill is to stop improvising. When the user says "帮我截图",
the default should be a clean in-app browser workflow.

## Core Rule

Prefer the Codex in-app browser.

Do not use the user's personal browser by default. Only use it when:

- the user explicitly asks for their browser
- the task depends on their logged-in browser state
- the in-app browser cannot satisfy the requirement and you explain why

Do not silently switch from the in-app browser to the user's browser.

## Default CodexPilot Preview Target

Unless the user asks for another target, assume the manager preview is:

```text
workdir: /Users/huanglin/code/github/CodexPilot/apps/codex-pilot-manager
command: npm run preview:ui
url: http://127.0.0.1:1420/
```

Known top-level manager pages:

- `总览`
- `模型通道`
- `对话维护`
- `诊断`

When the user asks for several manager screenshots without more detail, start
with these pages.

## Required Workflow

Follow this order:

1. identify the target page or surface the user wants
2. check whether the correct local preview is already running
3. if needed, start or restart the preview service
4. open the target in the Codex in-app browser
5. verify the visible page state before capturing
6. ensure the capture size is appropriate
7. take the screenshot
8. save it to the requested destination
9. report back the saved file paths

Avoid duplicate preview servers when the expected page is already available.

## Size And Completeness Rules

This project's manager preview behaves like a fixed application canvas. If the
browser viewport is too small, screenshots will look incomplete even though the
page is healthy.

Treat these rules as mandatory:

- do not assume the default in-app browser viewport is large enough
- before capture, make sure the viewport can fully show the intended UI
- if the requested result is a complete single-page image, do not default to a
  viewport-only screenshot
- prefer `fullPage` capture or an exact main-content clip for complete page
  images
- if the user asks for a focused detail, crop deliberately; do not rely on
  accidental viewport clipping

If a screenshot cuts off right-side actions, lower content, or fixed-width page
panels, fix the viewport or capture mode and retake it.

## Cleanliness Rules

Project screenshots should be clean by default.

Avoid capturing:

- translation banners
- account menus
- extension popups
- unrelated browser UI noise
- personal tabs or browser state

Capture the intended page state, not a transitional state.

## Output Rules

Honor the user's requested destination first.

Default destinations:

- explicit user path: save there
- `Downloads`: save there
- repo upload request: save into a meaningful repository image directory

Prefer semantic names such as:

- `codexpilot-overview.png`
- `codexpilot-provider.png`
- `codexpilot-sessions.png`
- `codexpilot-diagnostics.png`

Use timestamp suffixes only when the user wants versioned files or when you
need to avoid name collisions.

## Failure Handling

If something goes wrong:

- preview unavailable: start or restart it, then retry
- write blocked at final destination: save to a permitted temporary location,
  then move it
- wrong page visible: correct the page state before capturing
- in-app browser insufficient: explain the reason before changing tools

## Notes For This Repository

- Reuse an existing preview service when it already serves the correct page.
- For manager screenshots, verify whether the user wants:
  - one complete page per image
  - several top-level pages
  - a cropped detail
- If the user says a screenshot is "不全", treat that as a capture-mode or
  viewport problem first, not as proof that the page itself is broken.
