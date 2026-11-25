# Central Logs — Backend

Rust HTTP API for managing projects, log repositories and viewing log files.
Only the API is implemented here; the frontend lives elsewhere.

## Architecture

Layers, dependencies point inward:

| Layer | Path | Contents |
|---|---|---|
| Domain | `src/domain` | Aggregates `Project`, `LogRepository`; value objects `Name`, `LogDirectory`, `RelativeLogPath`. No I/O. |
| Application | `src/application` | CQRS: `CommandService` (writes) and `QueryService` (reads); `AuthService` (login); ports `ProjectRepository`, `LogRepositoryRepository`, read models, `LogFileSystem`, `IdentityProvider`, `PendingLoginStore`, `SessionStore`. |
| Infrastructure | `src/infrastructure` | `PgStore` (projects, log repositories, login sessions and pending logins; PostgreSQL), `InMemoryStore` (tests only), `FsLogFileSystem` (log files), `OidcIdentityProvider` (identity provider). |
| Interfaces | `src/interfaces/http` | axum routes, DTOs, error mapping. |

Domain invariants are enforced by factories, so invalid objects cannot be built:
names are trimmed and 1–100 characters; directories must be absolute and must not contain `..`;
a log repository needs at least one unique directory; project names are unique.

## Run

Requires PostgreSQL. Settings are read from the environment or from a `.env` file in this directory.

```sh
cp .env.example .env          # then fill in the values
make migrate                  # creates the database if missing and applies migrations
make build                    # release build: target/release/central_logs_backend
cargo run                     # listens on 127.0.0.1:8080
CENTRAL_LOGS_ADDR=0.0.0.0:9000 cargo run    # custom address
```

Database settings (all required; the server exits at startup if one is missing or the database is unreachable):

| Variable | Meaning |
|---|---|
| `DB_HOST` | PostgreSQL host |
| `DB_PORT` | PostgreSQL port |
| `DB_NAME` | Database name |
| `DB_USERNAME` | Role used to connect |
| `DB_PASSWORD` | Password for that role (may be empty if the server trusts the connection) |

`cargo run --bin console -- migrate` (or `make migrate`) is the operator console. It creates the
database when it does not exist (the role needs `CREATEDB` for that), then applies pending migrations
from `migrations/`. Running it again is a no-op. The server never migrates on its own.

Login needs these variables; the server exits at startup if a required one is missing:

| Variable | Required | Default | Meaning |
|---|---|---|---|
| `CENTRAL_LOGS_OIDC_CLIENT_ID` | yes | | Client registered with the identity provider |
| `CENTRAL_LOGS_OIDC_REDIRECT_URI` | yes | | Backend callback URL, e.g. `https://logs.example.com/api/auth/callback`; must be registered with the provider |
| `CENTRAL_LOGS_OIDC_CLIENT_SECRET` | no | | Only for confidential clients; PKCE is always used |
| `CENTRAL_LOGS_OIDC_ISSUER` | no | `https://identity.nvnhan0810.com` | Issuer; discovery is read from `{issuer}/.well-known/openid-configuration` |
| `CENTRAL_LOGS_FRONTEND_URL` | no | `http://localhost:3000` | Frontend origin: where the browser is sent after login, and the only origin allowed to call the API cross-origin (CORS with credentials). Origin only, no path. |
| `CENTRAL_LOGS_COOKIE_SECURE` | no | `true` | Set `false` only for plain-HTTP local development |

All state is stored in PostgreSQL: projects, log repositories, login sessions and pending logins.
A restart does not sign users out. Migration `0002` adds the session tables, so run `make migrate`
before starting a server that includes this change; the server does not migrate on its own.

## Test

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

PostgreSQL tests are ignored by default. They truncate tables, so they only run against a database
whose name ends in `_test`:

```sh
DB_NAME=central_logs_test cargo test --test postgres_store -- --ignored --test-threads=1
```

Tests are written test-first: domain rules, command/query use cases against fakes,
file-system behaviour on real temp directories (including a ~3.5 MB paging round-trip and
symlink escape), and end-to-end API tests through the real router.

## API

All bodies are JSON. Errors look like `{"error": {"code": "...", "message": "..."}}`.

| Method | Path | Purpose | Success |
|---|---|---|---|
| GET | `/api/projects` | List projects (sorted by name) | 200 |
| POST | `/api/projects` | Create `{ "name", "description"? }` | 201 `{ "id" }` |
| GET | `/api/projects/{id}` | Get one project | 200 |
| GET | `/api/projects/{id}/log-repositories` | List log repositories of a project | 200 |
| POST | `/api/projects/{id}/log-repositories` | Create `{ "name", "directories": [abs paths] }`; every directory must exist | 201 `{ "id" }` |
| GET | `/api/log-repositories/{id}/entries?directory=&path=` | List children of `path` inside one registered directory (`path` optional) | 200 |
| GET | `/api/log-repositories/{id}/file?directory=&path=&offset=&max_lines=&max_bytes=` | Read one page of a log file | 200 |
| GET | `/api/auth/login?return_to=` | Start login: redirects (302) to the identity provider | 302 |
| GET | `/api/auth/callback?code=&state=` | Identity provider redirect target: sets the session cookie, redirects (302) to the frontend | 302 |
| GET | `/api/auth/me` | Current user `{ subject, email, name }` | 200 / 401 |
| POST | `/api/auth/logout` | End the session and clear the cookie | 204 |

### Login (frontend integration)

Login is an OpenID Connect authorization code flow with PKCE, run by the backend:

1. The frontend navigates the browser to `/api/auth/login?return_to=/some/page` (a same-site relative path only; anything else is 400).
2. The backend redirects to the identity provider. The user signs in there.
3. The provider redirects to `/api/auth/callback`. The backend exchanges the code, reads the user's claims from `userinfo`, creates a session, sets cookie `central_logs_session` (`HttpOnly`, `SameSite=Lax`, 8 hours) and redirects to `CENTRAL_LOGS_FRONTEND_URL` + `return_to`.
4. The frontend calls `GET /api/auth/me` directly with `fetch(url, { credentials: "include" })` to know who is signed in. A 401 means sign in again. The backend answers CORS for the `CENTRAL_LOGS_FRONTEND_URL` origin only, with `Access-Control-Allow-Credentials: true`; other origins get no grant.
5. `POST /api/auth/logout` ends the session.

Errors use the usual shape. `400 invalid_request` covers a bad or reused `state`, an expired login, or a provider-reported error. `502 identity_unavailable` means the provider could not be reached or rejected the exchange; details are not returned to the client.

Notes:
- The browser sends the session cookie to the API only when the frontend and backend are same-site (for example both on `localhost`, or sub-domains of one domain). `SameSite=Lax` is unchanged, so mixing `localhost` and `127.0.0.1` will not work.
- Login `state` is single-use and expires after 10 minutes. Sessions expire after 8 hours.
- The ID token signature is not verified. Claims come from `userinfo`, which is called over TLS with the access token received directly from the provider's token endpoint.
- Existing `/api/projects` and `/api/log-repositories` routes do not require a session yet.

### Reading large log files

`/file` never loads the whole file. It seeks to `offset` (bytes) and returns at most
`max_lines` lines (default 200, max 5000) and `max_bytes` bytes of content (default 256 KiB, max 4 MiB).

Response:

```json
{
  "file": { "size": 3488895, "modified_unix": 1795601490 },
  "start_offset": 0,
  "next_offset": 19,
  "eof": false,
  "lines": [ { "text": "line 1", "truncated": false } ]
}
```

- To read a file from the start, call with `offset=0`, then repeat with `offset=next_offset` until `eof` is `true`.
- `offset` is a byte offset. If it lands mid-line, the first returned line is the tail of that line; sequential paging from `0` never has this issue.
- A single line larger than the byte budget is returned in pieces, with `truncated: true` on each piece except the last.
- Invalid UTF-8 is replaced with U+FFFD instead of failing the request.
- `offset` beyond end of file returns 400.

### Security

- `directory` must be one of the directories registered to the log repository, otherwise 404.
- `path` must be relative and cannot contain `..`.
- Paths are canonicalised; symlinks that resolve outside the registered directory are treated as not found.
- Session ids, login `state` and PKCE verifiers are about 244 random bits (two v4 UUIDs). Session ids are sent only in an `HttpOnly` cookie. Login redirects accept only same-site relative `return_to` paths.
