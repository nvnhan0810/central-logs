# Central Logs

Backend in `backend/` (Rust, PostgreSQL), frontend in `frontend/` (Vue 3, Vite).
See each directory's README for the details.

## Local test environment (Docker Compose)

`docker-compose.yml` runs the test suites in containers, with a PostgreSQL service for the
backend's database tests. Requires Docker with Compose v2. Nothing needs to be installed on the host.

```sh
docker compose run --rm backend-test     # clippy, cargo test, PostgreSQL tests (ignored by default)
docker compose run --rm frontend-test    # typecheck, lint, format check, vitest
docker compose up -d postgres            # PostgreSQL only, on 127.0.0.1:5432
docker compose down -v                   # stop everything and delete the database volume
```

- `backend-test` starts `postgres` and waits until it is healthy. The PostgreSQL tests run against the
  database `central_logs_test`, which the test fixture creates by running migrations.
- The source directories are mounted into the containers, so edits are picked up on the next run.
  Build output and dependencies are kept in named volumes (`backend-target`, `frontend-node-modules`,
  and the Cargo and npm caches), so the first run is slow and later runs reuse them.
- If port 5432 is already used on the host, set `CENTRAL_LOGS_DB_PORT` to another port, for example
  `CENTRAL_LOGS_DB_PORT=5433 docker compose up -d postgres`. The test containers always reach the
  database over the Compose network and are not affected.
- The database credentials in `docker-compose.yml` are for local use only.
