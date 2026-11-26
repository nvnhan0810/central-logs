-- Refresh-token support. `access_expires_at_ms` is when the current access token stops being trusted;
-- after that the next request refreshes it. `refresh_credentials` is the refresh token sealed with
-- the session key (AES-256-GCM), or NULL when the identity provider issued none.
-- Sessions that exist before this migration get access_expires_at_ms = 0, so they are signed out once
-- on the next request and must sign in again.
ALTER TABLE auth_sessions
    ADD COLUMN access_expires_at_ms BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN refresh_credentials TEXT;
