# Documentation Coverage Audit

## Remaining narrative gaps

- **nowhere-actors/src/builder.rs** — build on the new module docs with examples covering reservation order, registry lookups, and shutdown choreography.
- **nowhere-actors/src/system.rs** — elaborate on the shutdown broadcast contract and cancellation ordering beyond the newly added overview.
- **nowhere-actors/src/store.rs** — expand documentation on write throttling, watcher semantics, and SQL helpers; add diagrams of artifact flows.
- **nowhere-actors/src/twitter.rs** — document pagination, retry strategy, and interaction with the store/LLM pipeline in more depth.
- **nowhere-social/src/twitter/** — complement the module stubs with concrete guidance on pagination tokens, rate limits, and normalization expectations.
- **nowhere-config/src/lib.rs** — add schema diagrams/examples for `nowhere.yaml`, describe precedence rules, and note environment expansion edge cases.
- **nowhere-tui/src/** — document how commands, feeders, views, and transcript modules collaborate with the actor runtime.
- **nowhere-runtime/src/lib.rs** — flesh out cancellation behavior, multi-thread configuration, and downstream integration patterns.

## Test Coverage Follow-ups

The following `FIXME` markers were added while auditing codepaths that currently lack explicit tests:

- `nowhere-config/src/lib.rs#L75` — cover recursive environment-variable expansion across nested values.
- `nowhere-actors/src/rate.rs#L78` — add unit tests for burst capacity, refill timing, and concurrent `Acquire` calls.
- `nowhere-actors/src/store.rs#L31` — build integration tests covering claim inserts, artifact upserts, and watcher notifications so async spawns are validated.
- `nowhere-actors/src/twitter.rs#L50` — verify timestamp conversion error paths using boundary `DateTime<Utc>` values.

These should be tackled alongside future documentation passes to keep behavior well-understood and regression-resistant.
