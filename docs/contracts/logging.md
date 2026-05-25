# Logging Contract

- Runtime diagnostics should go through `codex_pilot_core::diagnostic_log`.
- Do not add new runtime `println!` calls in application code.
- Cargo build-script directives such as `println!("cargo:...")` in `build.rs`
  are allowed and are not treated as runtime logging violations.
