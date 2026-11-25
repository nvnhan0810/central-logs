import { describe, expect, it } from 'vitest';

import { ApiClient, ApiError, type FetchLike } from '@/infrastructure/http/api-client';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function recordingFetch(response: Response): {
  fetch: FetchLike;
  calls: [string, RequestInit | undefined][];
} {
  const calls: [string, RequestInit | undefined][] = [];
  const fetch: FetchLike = (input, init) => {
    calls.push([input, init]);
    return Promise.resolve(response.clone());
  };
  return { fetch, calls };
}

describe('ApiClient', () => {
  it('joins the base URL and strips trailing slashes', () => {
    const client = new ApiClient({ baseUrl: 'https://logs.example.com/api///' });
    expect(client.url('/auth/me')).toBe('https://logs.example.com/api/auth/me');
  });

  it('sends cookies and JSON accept header', async () => {
    const { fetch, calls } = recordingFetch(jsonResponse(200, { ok: true }));
    const client = new ApiClient({ baseUrl: '/api', fetch });

    await client.request('GET', '/auth/me');

    expect(calls[0]).toEqual([
      '/api/auth/me',
      { method: 'GET', credentials: 'include', headers: { Accept: 'application/json' } },
    ]);
  });

  it('returns undefined for 204 No Content', async () => {
    const { fetch } = recordingFetch(new Response(null, { status: 204 }));
    const client = new ApiClient({ baseUrl: '/api', fetch });

    await expect(client.request('POST', '/auth/logout')).resolves.toBeUndefined();
  });

  it('throws ApiError with the backend error code and message', async () => {
    const { fetch } = recordingFetch(
      jsonResponse(401, { error: { code: 'unauthenticated', message: 'sign in required' } }),
    );
    const client = new ApiClient({ baseUrl: '/api', fetch });

    const error: unknown = await client.request('GET', '/auth/me').catch((e: unknown) => e);

    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 401,
      code: 'unauthenticated',
      message: 'sign in required',
    });
  });

  it('falls back to an unknown code when the error body is not the API shape', async () => {
    const { fetch } = recordingFetch(new Response('<html>bad gateway</html>', { status: 502 }));
    const client = new ApiClient({ baseUrl: '/api', fetch });

    const error: unknown = await client.request('GET', '/auth/me').catch((e: unknown) => e);

    expect(error).toMatchObject({ status: 502, code: 'unknown' });
  });
});
