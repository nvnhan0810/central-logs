/** A project, as listed by the backend. */
export interface Project {
  readonly id: string;
  readonly name: string;
  readonly description: string;
}

/** A sub-project in the backend: a named set of absolute directories that hold log files. */
export interface LogRepository {
  readonly id: string;
  readonly projectId: string;
  readonly name: string;
  readonly directories: readonly string[];
}

/** Input for creating a project. */
export interface NewProject {
  readonly name: string;
  readonly description: string;
}

/** Input for creating a sub-project under a project. Each directory must be an existing absolute path. */
export interface NewLogRepository {
  readonly name: string;
  readonly directories: readonly string[];
}

export interface LogEntry {
  readonly name: string;
  /** Path from the registered directory root, e.g. `2026/11/app.log`. */
  readonly relativePath: string;
  readonly kind: 'file' | 'directory';
  readonly size: number | null;
}

export interface LogLine {
  readonly text: string;
  readonly truncated: boolean;
}

/** One page of a log file, starting at `startOffset` (bytes). */
export interface FileChunk {
  readonly size: number;
  readonly modifiedUnix: number | null;
  readonly startOffset: number;
  readonly nextOffset: number;
  readonly eof: boolean;
  readonly lines: readonly LogLine[];
}

/**
 * Port: everything the logs screen needs from the backend.
 * Infrastructure implements it over HTTP; the application never sees fetch or JSON.
 */
export interface LogGateway {
  listProjects(): Promise<readonly Project[]>;
  listLogRepositories(projectId: string): Promise<readonly LogRepository[]>;
  /** Creates a project and returns its id. */
  createProject(input: NewProject): Promise<string>;
  /** Creates a sub-project under `projectId` and returns its id. */
  createLogRepository(projectId: string, input: NewLogRepository): Promise<string>;
  /** Children of `relativePath` inside one registered `directory` (`''` is the directory itself). */
  listEntries(
    repositoryId: string,
    directory: string,
    relativePath: string,
  ): Promise<readonly LogEntry[]>;
  readChunk(
    repositoryId: string,
    directory: string,
    relativePath: string,
    offset: number,
  ): Promise<FileChunk>;
}
