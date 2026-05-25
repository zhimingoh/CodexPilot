# Subprocess Contract

- Runtime subprocess calls must use the repository subprocess integration rule in
  `codex_pilot_core::windows_integration`.
- New runtime `Command::new(...)` callsites are allowed only when the same path
  explicitly applies the Windows no-window helper before `spawn`, `status`, or
  `output`.
- The authoritative enforcement entry point is
  `scripts/check-windows-hygiene.sh`.
- Build scripts and other non-runtime tooling are outside this runtime
  subprocess rule unless they launch user-visible app behavior.
