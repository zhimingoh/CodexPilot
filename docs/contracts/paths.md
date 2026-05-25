# Paths Contract

- New product path resolution should reuse existing helpers such as
  `codex_pilot_core::app_paths`.
- Avoid scattering platform-specific path joins or home-directory guesses across
  unrelated modules.
- When a new path rule cannot fit existing helpers, add or extend a focused path
  abstraction instead of inlining more platform conditionals.
