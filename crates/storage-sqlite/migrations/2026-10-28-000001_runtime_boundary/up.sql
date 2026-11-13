-- Local admission metadata, deliberately outside device synchronization.
CREATE TABLE runtime_boundary (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    purpose TEXT NOT NULL CHECK (purpose IN ('standard', 'fictional', 'personal')),
    boundary_version INTEGER NOT NULL CHECK (boundary_version > 0)
);
