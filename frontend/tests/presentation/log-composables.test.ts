import { effectScope, type EffectScope } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type {
  FileChunk,
  LogEntry,
  LogGateway,
  LogRepository,
} from '@/application/logs/log-gateway';
import { useLogFile } from '@/presentation/logs/use-log-file';
import { useLogTree } from '@/presentation/logs/use-log-tree';

/** A file that can grow, served by offset like the backend. */
class FakeFiles {
  content = '';
  failNext = false;

  chunk(offset: number): FileChunk {
    if (offset > this.content.length) {
      throw new Error('offset is beyond end of file');
    }
    const text = this.content.slice(offset);
    const lines = text
      .split('\n')
      .filter((l) => l !== '')
      .map((text) => ({ text, truncated: false }));
    return {
      size: this.content.length,
      modifiedUnix: null,
      startOffset: offset,
      nextOffset: this.content.length,
      eof: true,
      lines,
    };
  }
}

function gatewayFor(
  files: FakeFiles,
  entries: LogEntry[] = [],
): LogGateway & { listCalls: number } {
  const gateway = {
    listCalls: 0,
    listProjects: () => Promise.reject(new Error('unused')),
    listLogRepositories: () => Promise.reject(new Error('unused')),
    createProject: () => Promise.reject(new Error('unused')),
    createLogRepository: () => Promise.reject(new Error('unused')),
    listEntries: () => {
      gateway.listCalls++;
      return Promise.resolve(entries);
    },
    readChunk: (_repo: string, _dir: string, _path: string, offset: number) => {
      if (files.failNext) {
        files.failNext = false;
        return Promise.reject(new Error('network down'));
      }
      return Promise.resolve(files.chunk(offset));
    },
  };
  return gateway;
}

/** Runs a composable inside an effect scope so its cleanup can be tested. */
function runIn<T>(scope: EffectScope, factory: () => T): T {
  const result = scope.run(factory);
  if (result === undefined) {
    throw new Error('effect scope is inactive');
  }
  return result;
}

const TARGET = { repositoryId: 'r1', directory: '/var/log/api', relativePath: 'app.log' };

afterEach(() => {
  vi.useRealTimers();
});

describe('useLogFile', () => {
  it('reads the file, then appends only new lines on each poll', async () => {
    vi.useFakeTimers();
    const files = new FakeFiles();
    files.content = 'one\ntwo\n';
    const scope = effectScope();
    const file = runIn(scope, () => useLogFile(gatewayFor(files), 1000));

    await file.open(TARGET);
    expect(file.status.value).toBe('live');
    expect(file.lines.value.map((l) => l.text)).toEqual(['one', 'two']);

    files.content += 'three\n';
    await vi.advanceTimersByTimeAsync(1000);
    expect(file.lines.value.map((l) => l.text)).toEqual(['one', 'two', 'three']);

    scope.stop();
  });

  it('keeps the file open and reports the error, then recovers on the next poll', async () => {
    vi.useFakeTimers();
    const files = new FakeFiles();
    files.content = 'one\n';
    const scope = effectScope();
    const file = runIn(scope, () => useLogFile(gatewayFor(files), 1000));
    await file.open(TARGET);

    files.content += 'two\n';
    files.failNext = true;
    await vi.advanceTimersByTimeAsync(1000);
    expect(file.status.value).toBe('error');
    expect(file.error.value).toBe('network down');

    await vi.advanceTimersByTimeAsync(1000);
    expect(file.status.value).toBe('live');
    expect(file.error.value).toBeNull();
    expect(file.lines.value.map((l) => l.text)).toEqual(['one', 'two']);

    scope.stop();
  });

  it('stops polling and clears state when closed', async () => {
    vi.useFakeTimers();
    const files = new FakeFiles();
    files.content = 'one\n';
    const scope = effectScope();
    const file = runIn(scope, () => useLogFile(gatewayFor(files), 1000));
    await file.open(TARGET);

    file.close();
    expect(file.status.value).toBe('idle');
    expect(file.lines.value).toEqual([]);

    files.content += 'late\n';
    await vi.advanceTimersByTimeAsync(5000);
    expect(file.lines.value).toEqual([]);

    scope.stop();
  });

  it('ignores a slow read that finishes after another file was opened', async () => {
    let release: (() => void) | undefined;
    const slow: LogGateway = {
      ...gatewayFor(new FakeFiles()),
      readChunk: (_r, _d, path) =>
        new Promise<FileChunk>((resolve) => {
          release = () => {
            resolve({
              size: 4,
              modifiedUnix: null,
              startOffset: 0,
              nextOffset: 4,
              eof: true,
              lines: [{ text: `stale ${path}`, truncated: false }],
            });
          };
        }),
    };
    const scope = effectScope();
    const file = runIn(scope, () => useLogFile(slow, 1000));

    const first = file.open(TARGET);
    file.close();
    release?.();
    await first;

    expect(file.lines.value).toEqual([]);
    expect(file.status.value).toBe('idle');
    scope.stop();
  });
});

describe('useLogTree', () => {
  const repository: LogRepository = {
    id: 'r1',
    projectId: 'p1',
    name: 'api',
    directories: ['/var/log/api'],
  };

  it('shows one root per registered directory and loads children on first expand', async () => {
    const gateway = gatewayFor(new FakeFiles(), [
      { name: 'app.log', relativePath: 'app.log', kind: 'file', size: 3 },
    ]);
    const scope = effectScope();
    const tree = runIn(scope, () => useLogTree(gateway));

    tree.show(repository);
    expect(tree.rows.value.map((r) => r.node.name)).toEqual(['api']);

    const [first] = tree.rows.value;
    if (first === undefined) {
      throw new Error('no root row');
    }
    const root = first.node;
    await tree.toggle(root);
    expect(gateway.listCalls).toBe(1);
    expect(tree.rows.value.map((r) => r.node.name)).toEqual(['api', 'app.log']);

    // Collapse and expand again: children are cached, no second call.
    await tree.toggle(root);
    await tree.toggle(root);
    expect(gateway.listCalls).toBe(1);
    scope.stop();
  });

  it('never opens a file through toggle', async () => {
    const gateway = gatewayFor(new FakeFiles());
    const scope = effectScope();
    const tree = runIn(scope, () => useLogTree(gateway));
    tree.show(repository);
    await tree.toggle({
      key: 'k',
      name: 'app.log',
      kind: 'file',
      directory: '/var/log/api',
      relativePath: 'app.log',
      expanded: false,
      loading: false,
      error: null,
      children: null,
    });
    expect(gateway.listCalls).toBe(0);
    scope.stop();
  });
});
