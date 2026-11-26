import { describe, expect, it } from 'vitest';

import { HttpLogGateway } from '@/infrastructure/logs/http-log-gateway';
import { ApiClient, ApiError, type FetchLike } from '@/infrastructure/http/api-client';

interface Sent {
  url: string;
  method: string | undefined;
  headers: Record<string, string> | undefined;
  body: unknown;
}

function gatewayReplying(status: number, body: unknown, sent: Sent[]): HttpLogGateway {
  const fetch: FetchLike = (url, init) => {
    sent.push({
      url,
      method: init?.method,
      headers: init?.headers as Record<string, string> | undefined,
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    });
    return Promise.resolve(
      new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
  };
  return new HttpLogGateway(new ApiClient({ baseUrl: '/api', fetch }));
}

describe('HttpLogGateway writes', () => {
  it('posts a project as JSON and returns the new id', async () => {
    const sent: Sent[] = [];
    const gateway = gatewayReplying(201, { id: 'p1' }, sent);

    await expect(
      gateway.createProject({ name: 'Payments', description: 'Thanh toán' }),
    ).resolves.toBe('p1');
    expect(sent[0]).toMatchObject({
      url: '/api/projects',
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: { name: 'Payments', description: 'Thanh toán' },
    });
  });

  it('posts a sub-project under its project with the directories', async () => {
    const sent: Sent[] = [];
    const gateway = gatewayReplying(201, { id: 'r1' }, sent);

    await expect(
      gateway.createLogRepository('p1', { name: 'api', directories: ['/var/log/api'] }),
    ).resolves.toBe('r1');
    expect(sent[0]).toMatchObject({
      url: '/api/projects/p1/log-repositories',
      method: 'POST',
      body: { name: 'api', directories: ['/var/log/api'] },
    });
  });

  it('keeps GET requests free of a body', async () => {
    const sent: Sent[] = [];
    const gateway = gatewayReplying(200, [], sent);

    await gateway.listProjects();
    expect(sent[0]?.headers).toEqual({ Accept: 'application/json' });
    expect(sent[0]?.body).toBeUndefined();
  });

  it('surfaces the backend message when the create is rejected', async () => {
    const sent: Sent[] = [];
    const gateway = gatewayReplying(
      409,
      { error: { code: 'conflict', message: 'project name already exists: Payments' } },
      sent,
    );

    const error = await gateway
      .createProject({ name: 'Payments', description: '' })
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 409,
      message: 'project name already exists: Payments',
    });
  });
});
