# Nowhere — Technical Infrastructure for Empirical Journalism

**Nowhere** is a terminal-first toolkit for building a more rigorous, empirical kind of journalism — one that privileges **evidence, provenance, and reproducibility** over narrative convenience. It’s designed as foundational infrastructure for those who believe that truth in the public sphere must be _verifiable, auditable, and open to scrutiny_.

Philosophically, Nowhere draws from Thomas Nagel’s _The View from Nowhere_: strive for objectivity without pretending to be free of perspective. Practically, that means **auditable pipelines**, **explicit trade-offs**, and **tooling that makes bias visible, not magical**.

---

## Highlights

- **Actor Runtime:** orchestrates acquisition → normalization → storage.
- **Pluggable LLM Backends:** OpenAI, Ollama, or self-hosted inference endpoints.
- **Social Ingest Workers:** Twitter/X integration with shared rate-limiting and durable SQLite storage.
- **Evidence Store:** SQLite tables + FTS views keep normalized artifacts, entities, and graph edges queryable with provenance.
- **Claim-Centric TUI:** define a claim, gather artifacts, review evidence interactively.
- **Modular Crates:** configuration, HTTP, drivers, and observability are isolated by design.

---

## Principles

- **Empirical First** — evidence before interpretation; provenance is a first-class field.
- **Reproducible** — same inputs → same outputs; explicit migrations and views.
- **Auditable** — structured logs and append-only records where it matters.
- **Terminal-Native** — favors clarity, text, and RTFM culture.
- **Modular** — integrations are pluggable; swap backends without rewriting pipelines.

---

## Quick Start

1. **Install prerequisites**
   - Rust toolchain (`rustup` on stable).
   - SQLite 3.43+ compiled with FTS5 (the schema uses virtual FTS tables).
   - Optional: `sqlx-cli` if you prefer `sqlx migrate run` over invoking `sqlite3`.

2. **Clone and stage environment**

   ```bash
   git clone <repo-url>
   cd nowhere
   cp .env.example .env
   ```

   Populate `.env` with `DATABASE_URL`, `OPENAI_API_KEY`, and `TWITTER_BEARER_TOKEN`. If you plan to use Ollama instead of OpenAI, disable or re-point the corresponding actor in `nowhere.yaml`.

3. **Export environment variables**

   ```bash
   set -a          # auto-export assignments
   source .env     # loads DATABASE_URL/OPENAI_API_KEY/TWITTER_BEARER_TOKEN
   set +a
   ```

   Any other method that exports the variables (e.g., `dotenvx run -f .env -- cargo …`) works; the app reads them via `std::env`.

4. **Initialize the database**

   ```bash
   sqlite3 nowhere.db < migrations/01_init.sql
   ```

   Ensure the path in `.env` matches the `DATABASE_URL` you plan to use (e.g., `sqlite://nowhere.db`).

5. **Configure actors**
   Edit `nowhere.yaml` to toggle actors, concurrency, and model settings. Secrets can stay in env vars because `${VAR}` expressions are expanded at load time.

6. **Run the TUI**

   ```bash
   cargo run -p nowhere-app
   ```

   Launch this in a true terminal (not the VS Code integrated preview) so crossterm can switch to the alternate screen. Use `/claim <text>` to start an investigation, then chat normally to question the collected evidence.

---

## Runtime Overview

- `nowhere-app` wires the system together via `tether.rs`, spinning up the rate limiter, SQLite store, configured LLMs, Twitter workers, and the Ratatui interface.
- When you create a claim, the TUI persists it, checks for prior artifacts, and asks the LLM to build a Twitter search query. Results are fetched by `TwitterSearchActor`, normalized by `LlmActor`, and written to SQLite (`StoreActor`) with entities and FTS entries.
- Follow-up questions are routed to `ChatLlmActor`, which pulls the most relevant artifacts/entities through FTS, instructs the LLM to answer with citations (`[A:artifact_id]`, `[E:entity_id]`), and streams the response back into the transcript.
- A broadcast shutdown handle coordinates orderly teardown, so `Ctrl+C` exits cleanly.

---

## Configuration Notes

- `nowhere.yaml` describes each actor (kind, id, concurrency, provider config). `${ENV_VAR}` expressions are expanded before deserialization, so you can keep tokens out of the file.
- Rate policies live in `nowhere-app/src/tether.rs`; adjust the `RateMsg::Upsert` calls if your environment can sustain higher throughput.
- The SQLite schema in `migrations/01_init.sql` sets up normalized artifacts, entities, evidence graph edges, and FTS hooks. Ensure your SQLite build ships with FTS5 enabled or the virtual table creation will fail.
- Logs default to `~/.local/share/nowhere/YYYY-MM-DD/nowhere.log`. Override via `NOWHERE_LOG_DIR` or set `RUST_LOG` for verbose tracing.

---

## Project Layout

| Directory          | Purpose                                                         |
| ------------------ | --------------------------------------------------------------- |
| `nowhere-app/`     | Terminal entrypoint; boots actors via `tether.rs`               |
| `nowhere-actors/`  | Actor framework, rate limiter, store, LLM + Twitter workers     |
| `nowhere-tui/`     | Ratatui-based claim/chat interface, command parsing, feeders    |
| `nowhere-config/`  | YAML/env loader for actor specs with `${VAR}` interpolation     |
| `nowhere-runtime/` | Tokio runtime wrapper and cancellation handles                  |
| `nowhere-common/`  | Shared observability helpers and workspace-wide types           |
| `nowhere-llm/`     | LLM clients (OpenAI, Ollama) implementing `LlmClient`           |
| `nowhere-social/`  | Twitter API client + response types                             |
| `nowhere-http/`    | Hardened HTTP client with retries and structured logging        |
| `nowhere-web/`     | Brave search client, browser primitives, HTML extraction        |
| `nowhere-drivers/` | Browser automation driver + stealth heuristics                  |
| `migrations/`      | SQLite schema, triggers, FTS maintenance, evidence graph tables |
| `docs/`            | Diagrams and the GitHub Pages site                              |

---

## Roadmap — Toward Evidence-First Journalism

Nowhere v0 is the runtime and TUI foundation. The long-term goal is to evolve it into a **complete epistemic infrastructure** for evidence-driven reporting and analysis:

- **Evidence Graph v1 → v2:** formalize `Record`, `Link`, and `Claim` into typed, queryable graph structures with explicit provenance edges, weighting, and inference support.
- **Evidence-Trained Models:** train lightweight LLMs on the evidence graph itself — producing models that _understand provenance_ rather than hallucinate context.
- **Reproducible Pipelines:** allow users to replay or branch entire investigations deterministically (same inputs → same conclusions).
- **Distributed Graph Store:** replace local SQLite with distributed, cryptographically signed event storage (append-only, verifiable).
- **Trusted Execution:** support offline/air-gapped verification and reproducible builds for secure reporting environments.
- **Schema Evolution:** use versioned evidence types to enable longitudinal truth tracking across time and sources.

In other words: **build the public ledger of empirical journalism** — where every claim, quote, and source is anchored to auditable evidence.

---

## Security & Privacy

- Keep credentials in `.env` or a secret manager; never commit them.
- Don’t publish investigation-specific logs or databases; this repo is clean by default.
- Rate limits and budgets live in `tether.rs`; adjust per environment.

---

## Website

The `docs/` folder hosts the GitHub Pages site with a minimal, terminal aesthetic.
To publish:

1. In GitHub → **Settings → Pages**
2. Choose “Deploy from a branch”
3. Source: `main`, Folder: `/docs`

---

## Contributing

PRs are welcome — keep them **focused, reproducible, and documented**.
Before opening a PR:

```bash
cargo fmt
cargo check
```

---

**v0 — public cut**
_A first step toward rigorous, verifiable, evidence-first journalism._
