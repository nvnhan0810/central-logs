import { describe, expect, it } from 'vitest';

import { HttpLogGateway } from '@/infrastructure/logs/http-log-gateway';
import { ApiClient, type FetchLike } from '@/infrastructure/http/api-client';

interface Call {
  url: string;
}

function recordingGateway(body: unknown, calls: Call[]): HttpLogGateway {
  const fetch: FetchLike = (url) => {
    calls.push({ url });
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
  };
  return new HttpLogGateway(new ApiClient({ baseUrl: '/api', fetch }));
}

describe('HttpLogGateway', () => {
  it('maps projects from snake_case JSON', async () => {
    const calls: Call[] = [];
    const gateway = recordingGateway([{ id: 'p1', name: 'Payments', description: '' }], calls);

    await expect(gateway.listProjects()).resolves.toEqual([
      { id: 'p1', name: 'Payments', description: '' },
    ]);
    expect(calls[0]?.url).toBe('/api/projects');
  });

  it('maps log repositories and reads project_id', async () => {
    const calls: Call[] = [];
    const gateway = recordingGateway(
      [{ id: 'r1', project_id: 'p1', name: 'api', directories: ['/var/log/api'] }],
      calls,
    );

    await expect(gateway.listLogRepositories('p1')).resolves.toEqual([
      { id: 'r1', projectId: 'p1', name: 'api', directories: ['/var/log/api'] },
    ]);
    expect(calls[0]?.url).toBe('/api/projects/p1/log-repositories');
  });

  it('encodes directory and path as query parameters', async () => {
    const calls: Call[] = [];
    const gateway = recordingGateway(
      [{ name: '2026', relative_path: '2026', kind: 'directory', size: null }],
      calls,
    );

    await expect(gateway.listEntries('r1', '/var/log/my app', '2026/11')).resolves.toEqual([
      { name: '2026', relativePath: '2026', kind: 'directory', size: null },
    ]);
    expect(calls[0]?.url).toBe(
      '/api/log-repositories/r1/entries?directory=%2Fvar%2Flog%2Fmy+app&path=2026%2F11',
    );
  });

  it('maps a file page with offsets and lines', async () => {
    const calls: Call[] = [];
    const gateway = recordingGateway(
      {
        file: { size: 12, modified_unix: 1795601490 },
        start_offset: 0,
        next_offset: 6,
        eof: false,
        lines: [{ text: 'line 1', truncated: false }],
      },
      calls,
    );

    await expect(gateway.readChunk('r1', '/var/log/api', 'app.log', 0)).resolves.toEqual({
      size: 12,
      modifiedUnix: 1795601490,
      startOffset: 0,
      nextOffset: 6,
      eof: false,
      lines: [{ text: 'line 1', truncated: false }],
    });
    expect(calls[0]?.url).toBe(
      '/api/log-repositories/r1/file?directory=%2Fvar%2Flog%2Fapi&path=app.log&offset=0',
    );
  });

  it('rejects a response that does not match the contract', async () => {
    const gateway = recordingGateway({ unexpected: true }, []);
    await expect(gateway.readChunk('r1', '/var/log/api', 'app.log', 0)).rejects.toThrow(
      'Unexpected response shape',
    );
  });
});
