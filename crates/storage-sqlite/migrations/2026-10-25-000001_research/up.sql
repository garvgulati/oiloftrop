-- Profile-local encrypted state. No Connect sync/outbox is implied.
CREATE TABLE research_plan (
    mode TEXT PRIMARY KEY NOT NULL CHECK(mode IN ('fictional','personal')),
    schema_version INTEGER NOT NULL CHECK(schema_version = 1),
    revision INTEGER NOT NULL CHECK(revision > 0),
    payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB)) <= 262144),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE research_runs (
    id TEXT PRIMARY KEY NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('fictional','personal')),
    schema_version INTEGER NOT NULL CHECK(schema_version = 1),
    status TEXT NOT NULL CHECK(status IN ('review','saved')),
    plan_revision INTEGER NOT NULL CHECK(plan_revision > 0),
    context_hash TEXT NOT NULL,
    evidence_hash TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    methodology_version TEXT NOT NULL,
    engine_build_id TEXT NOT NULL,
    assessment_as_of TEXT NOT NULL,
    payload TEXT NOT NULL CHECK(json_valid(payload) AND length(CAST(payload AS BLOB)) <= 2097152),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    saved_at TEXT,
    CHECK((status = 'review' AND saved_at IS NULL) OR (status = 'saved' AND saved_at IS NOT NULL))
);
CREATE INDEX research_runs_mode_status ON research_runs(mode,status,created_at,id);
