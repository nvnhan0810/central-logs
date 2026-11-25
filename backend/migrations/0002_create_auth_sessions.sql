-- Login sessions. `id` is the opaque session id held by the browser cookie.
-- Expiry is stored as milliseconds since the Unix epoch.
CREATE TABLE auth_sessions (
    id            TEXT PRIMARY KEY,
    subject       TEXT NOT NULL,
    email         TEXT,
    name          TEXT,
    expires_at_ms BIGINT NOT NULL
);

-- Logins started but whose callback has not arrived yet. `state` is the key the callback presents.
CREATE TABLE pending_logins (
    state         TEXT PRIMARY KEY,
    code_verifier TEXT NOT NULL,
    return_to     TEXT NOT NULL,
    expires_at_ms BIGINT NOT NULL
);
