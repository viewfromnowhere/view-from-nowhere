## Deterministic Capsules for `WebSearchActor`

This note sketches how to make the Brave-powered `WebSearchActor` participate in the deterministic, attestable runtime.

- **Execution model**  
  Treat each Brave query as a pure step: `(state_hash, WebSearchInput) -> (WebSearchOutput, EffectSet)`. Inputs include query string, rate-limiter token metadata, and any toggles (freshness, filters). Outputs are the ordered hit list plus metadata (result kind, snippet hashes, timestamps). Effects capture downstream writes (e.g., discovery queue insertions) as structured statements so they can be replayed or validated deterministically.

- **Algorithm overview**  
  ```
  // live execution
  clock, parents  <- scheduler.issue_ticket(actor_id)
  canonical_req   <- normalize_request(query, params, rate_key, seed)
  request_hash    <- hash(canonical_req)
  rate_meta       <- rate_limiter.acquire(rate_key)
  payload         <- edge.invoke("brave", canonical_req)        // HTTP in live mode, blob lookup in replay
  blob_hash       <- blob_store.commit(payload.raw_body)
  hits_digest     <- normalize_hits(payload.json)
  response_hash   <- hash(hits_digest, payload.headers, payload.status)
  effects         <- derive_effects(hits_digest)
  capsule         <- assemble(clock, parents, request_hash, response_hash, effects, blob_hash, telemetry)
  capsule_hash    <- hash(capsule)
  ledger.append(capsule_hash, capsule)
  effect_runtime.apply(effects)
  return hits_digest
  ```
  Replay reuses the same flow, substituting the HTTP call with a blob read and optionally applying effects against a shadow store for validation.

- **Provenance capsule schema**  
  ```
  Capsule {
    actor_id: "web_search.brave",
    invocation_id: Uuid,
    parent_invocations: Vec<CapsuleRef>,
    schedule_clock: LamportClock,
    request: {
      query: String,
      params: Map<String, Value>,
      rate_key: String,
      seed: u64,
    },
    response: {
      hits: Vec<HitDigest>,   // url, title hash, snippet hash, rank
      telemetry: {
        latency_ms: u32,
        source_trace: BraveHeaders,
      },
    },
    effects: Vec<Effect>,
    artifacts: BlobStoreRef,  // full JSON response optionally stored out-of-band
    hash: Digest,
  }
  ```
  By hashing structured fields (rather than raw JSON), replay only needs to recompute digests to check integrity. Large payloads live in a content-addressed blob store referenced from the capsule.

- **Canonical data structures**  
  - `WebSearchInput { query: CanonicalString, params: BTreeMap<String, CanonicalValue>, rate_key: String, seed: u64 }` ensures deterministic serialization and stable hashing.  
  - `BravePayload { status: u16, headers: CanonicalHeaders, raw_body: Bytes }` records exactly what Brave returned; `CanonicalHeaders` store lowercase names with sorted ordering.  
  - `HitDigest { url_hash: Digest, title_hash: Digest, snippet_hash: Digest, position: u16, vertical: HitKind }` captures only the derivation-relevant fields, keeping capsule hashes insensitive to formatting noise.

- **Deterministic scheduling**  
  The actor should execute under a coordinator that issues a Lamport clock tick before each Brave call. Capture the `clock` in the capsule and include the predecessor hashes; replay applies capsules in clock order, ensuring the same discovery ordering even when multiple actors run concurrently.

- **Edge determinism**  
  - **Randomness**: seed all sampling logic (if/when re-ranked) with the recorded `seed`.  
  - **Time**: replace direct `Instant`/`SystemTime` reads with a runtime-provided logical clock; record observed latency in telemetry but avoid using it for control flow.  
  - **HTTP drift**: persist the full Brave response (or its hash + blob ref). During replay, short-circuit network I/O by feeding the stored payload to the handler.

- **Side-effect journal**  
  Instead of writing discoveries immediately, queue them as `Effect { kind: "enqueue_discovery", payload: HitDigest }` within the capsule. After the capsule hash is sealed, the runtime applies effects transactionally. Replays can run in dry mode (validate the existing store matches) or apply effects to a shadow store.

- **Replay VM hook**  
  Implement a `WebSearchActor::replay(capsule, blob_store)` that:  
  1. Loads the stored Brave payload.  
  2. Recomputes `HitDigest` values.  
  3. Verifies the capsule hash.  
  4. Re-applies effects or checks them against current state.  
  Any divergence (missing blob, mismatched digest, different effect) becomes an audit failure.

- **Runtime integration (`nowhere-runtime`)**  
  The current `nowhere-app` bypasses `nowhere-runtime`, relying directly on Tokio. To support deterministic capsules:  
  - Extend `nowhere-runtime` with a scheduler API (`issue_ticket`) that returns `{clock, parent_hashes}` and exposes the shared cancellation token.  
  - Add a capsule ledger trait (`CapsuleStore`) and blob store abstraction so actors can persist provenance records without knowing storage details.  
  - Provide an effect dispatcher that applies queued effects only after a capsule is sealed, mirroring a commit log.  
  - Expose a replay harness (`ReplayVm`) that the CLI/TUI can invoke to validate or reapply capsules.  
  Once these primitives exist, `nowhere-app` should initialize `NowhereRuntime`, hand its handles into the actor builder, and ensure actors opt into deterministic scheduling.

- **Next steps**  
  1. Add instrumentation to the existing Brave client to emit the structured request/response.  
  2. Extend `nowhere-runtime` with capsule builder, scheduler, and effect commit APIs.  
  3. Build CLI tooling (`nowhere-runtime replay --actor web_search.brave --claim <id>`) to run the VM.  
  4. Promote capsule hashes to the evidence ledger so downstream logic can cite deterministic search steps.

This deterministic scaffold ensures Brave search results are reproducible, auditable units of computation, forming the foundation for the broader provenance system.
