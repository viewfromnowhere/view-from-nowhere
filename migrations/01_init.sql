-- Ensure FK enforcement in SQLite (you should also enable this in your app on every connection).
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS claim (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Main artifact table: 1 row per normalized artifact.
CREATE TABLE IF NOT EXISTS normalized_artifact (
  internal_id     TEXT PRIMARY KEY,                                  -- UUID as text
  external_id     TEXT NOT NULL UNIQUE,                              -- source-side identifier
  claim_relevance INTEGER NOT NULL CHECK (claim_relevance IN (0,1)), -- bool encoded as 0/1
  reasoning       TEXT NOT NULL DEFAULT '',
  provenance_info TEXT NOT NULL DEFAULT '',
  claim_id TEXT NOT NULL,

  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- Entities extracted from an artifact.
CREATE TABLE IF NOT EXISTS entity (
  id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))), -- synthetic UUID-like id
  article_id   TEXT NOT NULL,                                         -- FK -> normalized_artifact.internal_id
  external_id  TEXT NOT NULL,                                         -- source-side identifier for the entity (if any)
  name         TEXT NOT NULL,
  credibility  TEXT NOT NULL CHECK (credibility IN ('strong','weak','unknown')),
  reasoning    TEXT NOT NULL DEFAULT '',

  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),

  FOREIGN KEY (article_id) REFERENCES normalized_artifact(internal_id) ON DELETE CASCADE
);

-- Helpful indexes
CREATE INDEX IF NOT EXISTS idx_entity_article ON entity(article_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_article_external ON entity(article_id, external_id);
CREATE INDEX IF NOT EXISTS idx_entity_name ON entity(name);
CREATE INDEX IF NOT EXISTS idx_entity_credibility ON entity(credibility);

-- Touch updated_at on UPDATE
CREATE TRIGGER IF NOT EXISTS trg_normalized_artifact_updated
AFTER UPDATE ON normalized_artifact
FOR EACH ROW BEGIN
  UPDATE normalized_artifact
     SET updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
   WHERE internal_id = OLD.internal_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_entity_updated
AFTER UPDATE ON entity
FOR EACH ROW BEGIN
  UPDATE entity
     SET updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
   WHERE id = OLD.id;
END;


-- --------------------------------------------
-- 1) LLM-friendly read-only views
-- --------------------------------------------
CREATE VIEW IF NOT EXISTS v_artifact AS
SELECT
  internal_id,
  external_id,
  claim_relevance,
  substr(reasoning, 1, 2000)       AS reasoning,
  substr(provenance_info, 1, 2000) AS provenance_info,
  claim_id,
  created_at,
  updated_at
FROM normalized_artifact;

CREATE VIEW IF NOT EXISTS v_entity AS
SELECT
  id,
  article_id,         -- FK -> normalized_artifact.internal_id
  name,
  credibility,
  substr(reasoning, 1, 2000) AS reasoning,
  created_at,
  updated_at
FROM entity;

-- Helpful join for “show entities on an artifact”
CREATE VIEW IF NOT EXISTS v_artifact_entities AS
SELECT
  a.internal_id   AS artifact_id,
  a.external_id   AS artifact_external_id,
  e.id            AS entity_id,
  e.name          AS entity_name,
  e.credibility   AS entity_credibility
FROM v_artifact a
JOIN v_entity   e ON e.article_id = a.internal_id;

-- --------------------------------------------
-- 2) Optional: Full-Text Search for LLM retrieval
--     (requires SQLite with FTS5 enabled)
-- --------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS fts_artifact USING fts5(
  external_id,
  reasoning,
  provenance_info,
  claim_id,
  content='normalized_artifact',
  content_rowid='rowid'
);

-- Keep FTS in sync with normalized_artifact
CREATE TRIGGER IF NOT EXISTS trg_artifact_fts_ai
AFTER INSERT ON normalized_artifact
BEGIN
  INSERT INTO fts_artifact(rowid, external_id, reasoning, provenance_info, claim_id)
  VALUES (new.rowid, new.external_id, new.reasoning, new.provenance_info, new.claim_id);
END;

CREATE TRIGGER IF NOT EXISTS trg_artifact_fts_ad
AFTER DELETE ON normalized_artifact
BEGIN
  INSERT INTO fts_artifact(fts_artifact, rowid, external_id, reasoning, provenance_info, claim_id)
  VALUES ('delete', old.rowid, old.external_id, old.reasoning, old.provenance_info, old.claim_id);
END;

CREATE TRIGGER IF NOT EXISTS trg_artifact_fts_au
AFTER UPDATE ON normalized_artifact
BEGIN
  INSERT INTO fts_artifact(fts_artifact, rowid, external_id, reasoning, provenance_info, claim_id)
  VALUES ('delete', old.rowid, old.external_id, old.reasoning, old.provenance_info, old.claim_id);
  INSERT INTO fts_artifact(rowid, external_id, reasoning, provenance_info, claim_id)
  VALUES (new.rowid, new.external_id, new.reasoning, new.provenance_info, new.claim_id);
END;

-- --------------------------------------------
-- 3) Evidence Graph: edges between nodes
--    Nodes are:
--      - normalized_artifact.internal_id
--      - entity.id
-- --------------------------------------------
CREATE TABLE IF NOT EXISTS graph_edge (
  id          TEXT PRIMARY KEY,                                    -- deterministic UUID (e.g., v5 hash)
  src_id      TEXT NOT NULL,                                       -- artifact.internal_id or entity.id
  dst_id      TEXT NOT NULL,
  relation    TEXT NOT NULL CHECK (relation IN
                 ('supports','contradicts','mentions','same_event')),
  confidence  REAL NOT NULL CHECK (confidence BETWEEN 0.0 AND 1.0),
  rationale   TEXT NOT NULL,
  produced_by TEXT NOT NULL,                                       -- e.g., 'llm:v1'
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),

  -- Idempotence across re-proposals by the same producer
  UNIQUE (src_id, dst_id, relation, produced_by)
);

CREATE INDEX IF NOT EXISTS idx_graph_edge_src       ON graph_edge(src_id);
CREATE INDEX IF NOT EXISTS idx_graph_edge_dst       ON graph_edge(dst_id);
CREATE INDEX IF NOT EXISTS idx_graph_edge_relation  ON graph_edge(relation);
CREATE INDEX IF NOT EXISTS idx_graph_edge_producer  ON graph_edge(produced_by);

-- Convenience views for traversals
CREATE VIEW IF NOT EXISTS v_graph_mentions AS
SELECT ge.src_id AS artifact_id, ge.dst_id AS entity_id, ge.confidence, ge.rationale
FROM graph_edge ge
WHERE ge.relation = 'mentions';

CREATE VIEW IF NOT EXISTS v_graph_supports AS
SELECT ge.src_id AS src_artifact_id, ge.dst_id AS dst_artifact_id, ge.confidence, ge.rationale
FROM graph_edge ge
WHERE ge.relation = 'supports';

-- --------------------------------------------
-- 4) (Optional) Proposals table for LLM-suggested writes.
-- --------------------------------------------
CREATE TABLE IF NOT EXISTS proposal (
  id           TEXT PRIMARY KEY,                         -- UUID
  kind         TEXT NOT NULL CHECK (kind IN ('artifact','entity','link','update')),
  target_id    TEXT,                                     -- e.g., artifact.internal_id for updates
  payload_json TEXT NOT NULL,                            -- canonical change
  rationale    TEXT NOT NULL,
  status       TEXT NOT NULL CHECK (status IN ('pending','accepted','rejected'))
                 DEFAULT 'pending',
  created_by   TEXT NOT NULL DEFAULT 'llm',
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  reviewed_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_proposal_status ON proposal(status);
