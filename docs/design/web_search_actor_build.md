## `WebSearchActor` Build Notes

This note documents how the Brave-driven `WebSearchActor` should evolve from the current stub into a first-class participant in the Nowhere runtime. It complements `docs/design/deterministic_websearch.md` by focusing on the concrete actor wiring and runtime responsibilities rather than replay capsules alone.

### Existing Surface Area
- `nowhere-actors::SearchCmd` already models the upstream request payload that the TUI issues when it wants fresh web evidence (query string + date window + claim context).
- `nowhere-web::brave::client::BraveApi` contains a mostly-complete HTTP client (currently commented out) with helpers for page iteration, result ordering, and rate limiting.
- `nowhere-actors::store::StoreMsg::UpsertArtifact` accepts normalized items ready for persistence, and `RawArtifact` wraps the un-normalized payload we should emit before the LLM pass.
- `docs/design/deterministic_websearch.md` defines the provenance capsule schema and outlines scheduler integration; this doc assumes those primitives become available through `nowhere-runtime`.

### Message Contract Proposal
- Define `WebSearchMsg` in `nowhere-actors/src/brave.rs` with variants:
  - `Execute(SearchCmd)` – primary entry point invoked by the supervisor/builder.
  - `Replay { capsule: CapsuleRef }` – optional hook once deterministic replay lands.
  - `Shutdown` – optional graceful drain if we need to close HTTP clients.
- Publish the actor address under a stable registry key (e.g., `"web_search.brave"`) so both the orchestrator and any ad-hoc tools can reuse it without bespoke plumbing.

### Execution Flow (Live Mode)
1. **Rate Ticket**: Convert the incoming `SearchCmd` into a Brave-friendly request (including pagination + freshness heuristics) and acquire a token from the shared `RateLimiter` using a dedicated `RateKey`.
2. **HTTP Call**: Invoke `BraveApi::search_page` (initially single page, later paginated) and capture the raw JSON + headers for provenance. Lean on the existing helper to maintain Brave’s display order.
3. **Hit Normalization**:
   - Transform each hit into a `RawArtifact` with `external_id` derived from URL, claim metadata from the request, and the original Brave snippet tucked into `payload`.
   - Queue basic provenance fields (`source: "brave"`, `rank`, `vertical`, `retrieved_at`) that the LLM can later fold into reasoning.
4. **Downstream Dispatch**:
   - Send each `RawArtifact` to the normalization LLM (`LlmMsg::NormalizeArtifact`).
   - Optionally emit a compact `Discovery` event for future browser capture actors (e.g., queue URLs for headless retrieval).
5. **Capsule Assembly**: When deterministic runtime plumbing is available, wrap the step in a provenance capsule (request hash + response digest + effect journal) as described in the companion design doc.
6. **Observability**: Emit structured tracing spans (`web.brave.execute`) with tags for query hash, result count, pagination, rate-wait latency, and capsule hash once supported.

### Deterministic Runtime Hooks
- Require a `CapsuleStore`/`BlobStore` handle at construction so the actor can persist payloads before applying effects.
- Ensure all randomness is seeded via the runtime ticket (`seed`), and bubble the logical `LamportClock` into logs for reproducibility.
- Capture rate-limiter timing (wait duration, permit id) but avoid data-dependent branching on wall-clock times.

### Error Handling Expectations
- Map Brave HTTP/status errors into `anyhow::Error` and decide retry vs. failure in one place (likely within the actor, not the client).
- Deduplicate URLs across pages, but log when Brave returns malformed entries to aid hardening.
- Surface rate-limit exhaustion with backoff hints so the supervisor can pause or reschedule without collapsing the whole pipeline.

### Implementation Checklist
1. Restore and modernize `BraveApi` (lift the commented code, ensure it compiles against current crates, and add minimal unit coverage for the hit collection helpers).
2. Create `WebSearchActor` struct with dependencies injected (`BraveApi`, `Addr<RateLimiter>`, `Addr<LlmActor>`, optional `CapsuleStore`).
3. Implement `Actor for WebSearchActor`: handle `Execute` by performing the flow above, spawn normalization fan-out tasks, and respect shutdown signals.
4. Plumb actor registration in the builder (`nowhere-actors::builder`) so `nowhere-app` can request searches and wire rate limits from config.
5. Integrate deterministic capsule construction once `nowhere-runtime` exposes the scheduler and blob APIs; keep the live path compatible with today’s non-deterministic runtime until then.
6. Document configuration knobs (`BRAVE_TOKEN`, concurrency limits, result caps) in `README` / relevant markdown so operators know how to enable the actor.

Following these steps keeps the WebSearchActor aligned with the broader deterministic vision while delivering incremental value: first as a pragmatic Brave search worker, then as an auditable capsule producer.
