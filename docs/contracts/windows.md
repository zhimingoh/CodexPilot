# Windows Contract

- New or changed features that touch subprocesses, paths, permissions, or
  visible window behavior must consider Windows behavior explicitly.
- Windows validation is required for launch, reinject, restart, and focus-driven
  manager refresh behavior.
- CI can enforce build, test, and lint gates, but Windows screenshots remain a
  review/process requirement rather than a fully automatable rule.
