/** Error returned by the backend, shaped as `{ "error": { "code", "message" } }`. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

export interface ApiClientOptions {
  /** Prefix for every path, e.g. `http://localhost:8080/api` (called directly, with CORS). */
  readonly baseUrl: string;
  /** Injected for tests; defaults to the global `fetch`. */
  readonly fetch?: FetchLike;
}

const NO_CONTENT = 204;

export class ApiClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: FetchLike;

  constructor(options: ApiClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    // Wrapped so the global fetch is never called with a foreign `this`.
    this.fetchImpl = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  }

  /** URL for a path under the API base, e.g. `url('/auth/me')`. */
  url(path: string): string {
    return `${this.baseUrl}${path}`;
  }

  /**
   * Sends a request with cookies and returns the parsed JSON body (`undefined` for 204).
   * A `body` is sent as JSON.
   */
  async request(method: 'GET' | 'POST', path: string, body?: unknown): Promise<unknown> {
    const response = await this.fetchImpl(this.url(path), {
      method,
      credentials: 'include',
      headers:
        body === undefined
          ? { Accept: 'application/json' }
          : { Accept: 'application/json', 'Content-Type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });

    if (!response.ok) {
      throw await toApiError(response);
    }
    if (response.status === NO_CONTENT) {
      return undefined;
    }
    return (await response.json()) as unknown;
  }
}

async function toApiError(response: Response): Promise<ApiError> {
  const fallback = new ApiError(
    response.status,
    'unknown',
    response.statusText || 'Request failed',
  );
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return fallback;
  }
  if (!isErrorBody(body)) {
    return fallback;
  }
  return new ApiError(response.status, body.error.code, body.error.message);
}

interface ErrorBody {
  readonly error: { readonly code: string; readonly message: string };
}

function isErrorBody(value: unknown): value is ErrorBody {
  if (typeof value !== 'object' || value === null || !('error' in value)) {
    return false;
  }
  const error = value.error;
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    typeof error.code === 'string' &&
    'message' in error &&
    typeof error.message === 'string'
  );
}
