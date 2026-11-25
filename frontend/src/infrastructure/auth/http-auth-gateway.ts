import type { AuthGateway } from '@/application/auth/auth-gateway';
import type { User } from '@/domain/auth/user';
import { ApiError, type ApiClient } from '@/infrastructure/http/api-client';

import { isUserDto, toUser } from './user-dto';

/** Talks to the backend auth endpoints. See backend/README.md "Login (frontend integration)". */
export class HttpAuthGateway implements AuthGateway {
  private readonly client: ApiClient;

  constructor(client: ApiClient) {
    this.client = client;
  }

  async fetchCurrentUser(): Promise<User | null> {
    try {
      const body = await this.client.request('GET', '/auth/me');
      if (!isUserDto(body)) {
        throw new Error('Unexpected response from /api/auth/me');
      }
      return toUser(body);
    } catch (error) {
      // 401 is the normal "not signed in" answer, not a failure.
      if (error instanceof ApiError && error.status === 401) {
        return null;
      }
      throw error;
    }
  }

  loginUrl(returnTo: string): string {
    const query = new URLSearchParams({ return_to: returnTo });
    return `${this.client.url('/auth/login')}?${query.toString()}`;
  }

  async logout(): Promise<void> {
    await this.client.request('POST', '/auth/logout');
  }
}
