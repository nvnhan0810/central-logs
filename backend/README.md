# Central Logs — Backend

Rust HTTP API for managing projects, log repositories and viewing log files.
Only the API is implemented here; the frontend lives elsewhere.

## Architecture

Layers, dependencies point inward:

| Layer | Path | Contents |
|---|---|---|
| Domain | `src/domain` | Aggregates `Project`, `LogRepository`; value objects `Name`, `LogDirectory`, `RelativeLogPath`. No I/O. |
| Application | `src/application` | CQRS: `CommandService` (writes) and `QueryService` (reads); ports `ProjectRepository`, `LogRepositoryRepository`, read models, `LogFileSystem`. |
| Infrastructure | `src/infrastructure` | `InMemoryStore` (metadata), `FsLogFileSystem` (log files). |
| Interfaces | `src/interfaces/http` | axum routes, DTOs, error mapping. |

Domain invariants are enforced by factories, so invalid objects cannot be built:
names are trimmed and 1–100 characters; directories must be absolute and must not contain `..`;
a log repository needs at least one unique directory; project names are unique.

## Run

```sh
cargo run                                   # listens on 127.0.0.1:8080
CENTRAL_LOGS_ADDR=0.0.0.0:9000 cargo run    # custom address
```

Metadata (projects, log repositories) is kept **in memory** and is lost on restart.

## Test

```sh
cargo test
cargo clippy --all-targets -- -D warnings
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
