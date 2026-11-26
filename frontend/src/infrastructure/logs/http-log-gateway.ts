import type {
  FileChunk,
  LogEntry,
  LogGateway,
  LogRepository,
  NewLogRepository,
  NewProject,
  Project,
} from '@/application/logs/log-gateway';
import type { ApiClient } from '@/infrastructure/http/api-client';

import {
  asArray,
  asBoolean,
  asNullableNumber,
  asNumber,
  asObject,
  asString,
  asStringArray,
} from './wire';

/** Talks to the log endpoints of backend/README.md "API". Maps snake_case JSON to camelCase models. */
export class HttpLogGateway implements LogGateway {
  constructor(private readonly client: ApiClient) {}

  async listProjects(): Promise<readonly Project[]> {
    const body = await this.client.request('GET', '/projects');
    return asArray(body, 'projects').map(toProject);
  }

  async listLogRepositories(projectId: string): Promise<readonly LogRepository[]> {
    const body = await this.client.request(
      'GET',
      `/projects/${encodeURIComponent(projectId)}/log-repositories`,
    );
    return asArray(body, 'log repositories').map(toLogRepository);
  }

  async createProject(input: NewProject): Promise<string> {
    const body = await this.client.request('POST', '/projects', {
      name: input.name,
      description: input.description,
    });
    return asString(asObject(body, 'created project').id, 'created project.id');
  }

  async createLogRepository(projectId: string, input: NewLogRepository): Promise<string> {
    const body = await this.client.request(
      'POST',
      `/projects/${encodeURIComponent(projectId)}/log-repositories`,
      { name: input.name, directories: [...input.directories] },
    );
    return asString(asObject(body, 'created log repository').id, 'created log repository.id');
  }

  async listEntries(
    repositoryId: string,
    directory: string,
    relativePath: string,
  ): Promise<readonly LogEntry[]> {
    const body = await this.client.request(
      'GET',
      `/log-repositories/${encodeURIComponent(repositoryId)}/entries?${query({ directory, path: relativePath })}`,
    );
    return asArray(body, 'entries').map(toLogEntry);
  }

  async readChunk(
    repositoryId: string,
    directory: string,
    relativePath: string,
    offset: number,
  ): Promise<FileChunk> {
    const body = await this.client.request(
      'GET',
      `/log-repositories/${encodeURIComponent(repositoryId)}/file?${query({
        directory,
        path: relativePath,
        offset: String(offset),
      })}`,
    );
    return toFileChunk(body);
  }
}

function query(params: Record<string, string>): string {
  return new URLSearchParams(params).toString();
}

function toProject(value: unknown): Project {
  const o = asObject(value, 'project');
  return {
    id: asString(o.id, 'project.id'),
    name: asString(o.name, 'project.name'),
    description: asString(o.description, 'project.description'),
  };
}

function toLogRepository(value: unknown): LogRepository {
  const o = asObject(value, 'log repository');
  return {
    id: asString(o.id, 'log repository.id'),
    projectId: asString(o.project_id, 'log repository.project_id'),
    name: asString(o.name, 'log repository.name'),
    directories: asStringArray(o.directories, 'log repository.directories'),
  };
}

function toLogEntry(value: unknown): LogEntry {
  const o = asObject(value, 'entry');
  const kind = asString(o.kind, 'entry.kind');
  if (kind !== 'file' && kind !== 'directory') {
    throw new Error(`Unexpected response shape: entry.kind=${kind}`);
  }
  return {
    name: asString(o.name, 'entry.name'),
    relativePath: asString(o.relative_path, 'entry.relative_path'),
    kind,
    size: asNullableNumber(o.size, 'entry.size'),
  };
}

function toFileChunk(value: unknown): FileChunk {
  const o = asObject(value, 'file chunk');
  const file = asObject(o.file, 'file chunk.file');
  return {
    size: asNumber(file.size, 'file.size'),
    modifiedUnix: asNullableNumber(file.modified_unix, 'file.modified_unix'),
    startOffset: asNumber(o.start_offset, 'chunk.start_offset'),
    nextOffset: asNumber(o.next_offset, 'chunk.next_offset'),
    eof: asBoolean(o.eof, 'chunk.eof'),
    lines: asArray(o.lines, 'chunk.lines').map((line) => {
      const l = asObject(line, 'chunk line');
      return {
        text: asString(l.text, 'line.text'),
        truncated: asBoolean(l.truncated, 'line.truncated'),
      };
    }),
  };
}
