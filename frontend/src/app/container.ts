import { AuthService } from '@/application/auth/auth-service';
import { HttpAuthGateway } from '@/infrastructure/auth/http-auth-gateway';
import { ApiClient } from '@/infrastructure/http/api-client';

/** Composition root: the only place that knows which infrastructure is wired in. */
export interface AppServices {
  readonly auth: AuthService;
}

export function createAppServices(
  apiBaseUrl: string = import.meta.env.VITE_API_BASE_URL ?? '/api',
): AppServices {
  const client = new ApiClient({ baseUrl: apiBaseUrl });
  return {
    auth: new AuthService(new HttpAuthGateway(client)),
  };
}
