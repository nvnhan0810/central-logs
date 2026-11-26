import { describe, expect, it } from 'vitest';

import type { FileChunk, LogGateway } from '@/application/logs/log-gateway';
import { readFrom, type FileTarget } from '@/application/logs/read-from';

const TARGET: FileTarget = {
  repositoryId: 'r1',
  directory: '/var/log/api',
  relativePath: 'app.log',
};

/** Serves `content` in pages of at most `pageBytes`, like the backend does. */
function fakeGateway(content: string, pageBytes: number, calls: number[] = []): LogGateway {
  const page = (offset: number): FileChunk => {
    calls.push(offset);
    // Like the backend: a page ends on a line boundary (at least one whole line).
    let end = Math.min(content.length, offset + pageBytes);
    if (end < content.length) {
      const lastNewline = content.lastIndexOf('\n', end - 1);
      end = lastNewline >= offset ? lastNewline + 1 : content.indexOf('\n', offset) + 1;
    }
    const text = content.slice(offset, end);
    return {
      size: content.length,
      modifiedUnix: null,
      startOffset: offset,
      nextOffset: end,
      eof: end >= content.length,
      lines: text
        .split('\n')
        .filter((line) => line !== '')
        .map((line) => ({ text: line, truncated: false })),
    };
  };
  return {
    listProjects: () => Promise.reject(new Error('unused')),
    listLogRepositories: () => Promise.reject(new Error('unused')),
    listEntries: () => Promise.reject(new Error('unused')),
    readChunk: (_repo, _dir, _path, offset) => Promise.resolve(page(offset)),
  };
}

describe('readFrom', () => {
  it('reads every page until EOF and returns the lines in file order', async () => {
    const calls: number[] = [];
    const gateway = fakeGateway('alpha\nbeta\ngamma\n', 7, calls);

    const result = await readFrom(gateway, TARGET, 0);

    expect(result?.lines.map((l) => l.text)).toEqual(['alpha', 'beta', 'gamma']);
    expect(result?.nextOffset).toBe(17);
    expect(result?.size).toBe(17);
    expect(calls.length).toBeGreaterThan(1);
  });

  it('reads only newer content when given the previous offset', async () => {
    const gateway = fakeGateway('alpha\nbeta\ngamma\n', 1024);
    const result = await readFrom(gateway, TARGET, 6);
    expect(result?.lines.map((l) => l.text)).toEqual(['beta', 'gamma']);
  });

  it('returns an empty result at EOF', async () => {
    const gateway = fakeGateway('alpha\n', 1024);
    const result = await readFrom(gateway, TARGET, 6);
    expect(result?.lines).toEqual([]);
    expect(result?.nextOffset).toBe(6);
  });

  it('gives up with null when the caller is no longer interested', async () => {
    const gateway = fakeGateway('alpha\nbeta\n', 1024);
    await expect(readFrom(gateway, TARGET, 0, () => false)).resolves.toBeNull();
  });

  it('fails instead of looping when the backend does not advance', async () => {
    const stuck: LogGateway = {
      ...fakeGateway('', 1),
      readChunk: () =>
        Promise.resolve({
          size: 10,
          modifiedUnix: null,
          startOffset: 0,
          nextOffset: 0,
          eof: false,
          lines: [],
        }),
    };
    await expect(readFrom(stuck, TARGET, 0)).rejects.toThrow('did not advance');
  });
});
