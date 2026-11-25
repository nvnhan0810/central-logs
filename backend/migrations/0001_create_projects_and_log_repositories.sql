-- Projects group log repositories. Names are unique, as the domain requires.
CREATE TABLE projects (
    id          UUID PRIMARY KEY,
    name        VARCHAR(100) NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE log_repositories (
    id         UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       VARCHAR(100) NOT NULL
);

CREATE INDEX log_repositories_project_id_idx ON log_repositories (project_id);

-- Absolute host directories of a log repository. `position` keeps the order they were given in.
CREATE TABLE log_repository_directories (
    log_repository_id UUID NOT NULL REFERENCES log_repositories (id) ON DELETE CASCADE,
    position          INTEGER NOT NULL,
    path              TEXT NOT NULL,
    PRIMARY KEY (log_repository_id, position),
    UNIQUE (log_repository_id, path)
);
