import { AuthService } from '@/application/auth/auth-service';
import type { LogGateway } from '@/application/logs/log-gateway';
import { HttpAuthGateway } from '@/infrastructure/auth/http-auth-gateway';
import { HttpLogGateway } from '@/infrastructure/logs/http-log-gateway';
import { ApiClient } from '@/infrastructure/http/api-client';

/** Composition root: the only place that knows which infrastructure is wired in. */
export interface AppServices {
  readonly auth: AuthService;
  readonly logs: LogGateway;
}

/** Default backend address (backend/ listens on 127.0.0.1:8080). `localhost` keeps it same-site with the dev server. */
export const DEFAULT_API_BASE_URL = 'http://localhost:8080/api';

export function createAppServices(
  apiBaseUrl: string = import.meta.env.VITE_API_BASE_URL ?? DEFAULT_API_BASE_URL,
): AppServices {
  const client = new ApiClient({ baseUrl: apiBaseUrl });
  return {
    auth: new AuthService(new HttpAuthGateway(client)),
    logs: new HttpLogGateway(client),
  };
}
