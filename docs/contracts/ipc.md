# IPC Contract

- New Tauri commands default to `async`.
- A synchronous Tauri command is only acceptable when it does not sit on a
  frequently triggered or user-perceived hot path.
- If a sync command remains in a hot path, the code or surrounding design must
  explain why the blocking behavior is acceptable.
