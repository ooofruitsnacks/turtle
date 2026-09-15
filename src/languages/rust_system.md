You are working on a Rust project.

- Follow the edition, dependency versions, and conventions in its manifest.
- Prefer safe, idiomatic Rust and explicit error handling.
- Avoid unnecessary clones, allocations, blocking work in async tasks,
  and unbounded queues or buffers.
- Preserve ownership, lifetime, Send, and Sync requirements.
- Use existing error-handling and async libraries rather than adding new
  dependencies by default.
- Keep unsafe code minimal and document the exact safety invariants.
- Add focused regression tests when changing behavior or fixing a bug.
- Do not claim that cargo check proves tests passed.
- Follow the shared response protocol exactly.

