# Testing Guide

The workspace contains multiple crates with shared runtime behaviour. This guide captures the initial testing posture and highlights concrete next steps for unit, integration, and documentation tests.

## Goals

- Provide fast confidence for configuration parsing and environment interpolation.
- Document invariants for actor orchestration without touching core functionality.
- Grow fixture coverage around drivers that talk to external services.

## Getting Started

1. Run `cargo test --all --doc` to exercise doctests (including the new configuration example in `nowhere-config`).
2. Execute focused crates with `cargo test -p nowhere-config` while you iterate on loader behaviour.
3. Ensure `sqlx-cli` is installed if you plan to run migration smoke tests; the binary is optional but useful for verifying schema drift.

## Unit Tests

- Prefer small, isolated tests under each crate’s `tests/` directory or module-local `#[cfg(test)]` blocks.
- Mock rate-limiters and network backends via trait implementations; avoid hitting real APIs in CI.
- When dealing with `OnceLock` or other global state, add helper resets behind `cfg(test)` gates to keep tests hermetic.

## Integration Tests

- Place end-to-end orchestration tests in `nowhere-app/tests/` so they can spin up the runtime and verify actor wiring.
- Use temporary directories (`tempfile::TempDir`) for log outputs and SQLite databases to prevent cross-test interference.
- Capture structured logs with `tracing` subscribers configured for deterministic assertions.

## Future Work

- Add fixtures that validate `nowhere.yaml` interpolation for multiple actors and concurrency overrides.
- Introduce doctests for social client query builders once the HTTP layer stabilises.
- Extend migration verification with a golden SQLite database to detect schema regressions.
