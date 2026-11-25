import { describe, expect, it } from 'vitest';

import { HttpAuthGateway } from '@/infrastructure/auth/http-auth-gateway';
import { ApiClient, type FetchLike } from '@/infrastructure/http/api-client';

function gatewayWith(fetch: FetchLike): HttpAuthGateway {
  return new HttpAuthGateway(new ApiClient({ baseUrl: '/api', fetch }));
}

function respond(status: number, body?: unknown): FetchLike {
  return () =>
    Promise.resolve(
      body === undefined
        ? new Response(null, { status })
        : new Response(JSON.stringify(body), {
            status,
            headers: { 'Content-Type': 'application/json' },
          }),
    );
}

describe('HttpAuthGateway', () => {
  it('maps GET /api/auth/me to a User', async () => {
    const gateway = gatewayWith(
      respond(200, { subject: 'sub-1', email: 'a@example.com', name: null }),
    );

    await expect(gateway.fetchCurrentUser()).resolves.toEqual({
      subject: 'sub-1',
      email: 'a@example.com',
      name: null,
    });
  });

  it('returns null on 401 (not signed in)', async () => {
    const gateway = gatewayWith(
      respond(401, { error: { code: 'unauthenticated', message: 'sign in required' } }),
    );

    await expect(gateway.fetchCurrentUser()).resolves.toBeNull();
  });

  it('rejects a 200 body that does not match the user shape', async () => {
    const gateway = gatewayWith(respond(200, { subject: 42 }));

    await expect(gateway.fetchCurrentUser()).rejects.toThrow('Unexpected response');
  });

  it('rethrows non-401 failures', async () => {
    const gateway = gatewayWith(
      respond(502, {
        error: { code: 'identity_unavailable', message: 'identity provider is unavailable' },
      }),
    );

    await expect(gateway.fetchCurrentUser()).rejects.toMatchObject({ status: 502 });
  });

  it('builds the login URL with an encoded return_to', () => {
    const gateway = gatewayWith(respond(204));

    expect(gateway.loginUrl('/projects/1?tab=logs&x=a b')).toBe(
      '/api/auth/login?return_to=%2Fprojects%2F1%3Ftab%3Dlogs%26x%3Da+b',
    );
  });

  it('posts to logout', async () => {
    const seen: { url: string; method: string | undefined }[] = [];
    const gateway = gatewayWith((input, init) => {
      seen.push({ url: input, method: init?.method });
      return Promise.resolve(new Response(null, { status: 204 }));
    });

    await gateway.logout();

    expect(seen).toEqual([{ url: '/api/auth/logout', method: 'POST' }]);
  });
});
