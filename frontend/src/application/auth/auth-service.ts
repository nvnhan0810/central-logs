import type { AuthState } from '@/domain/auth/auth-state';
import { UNKNOWN_AUTH_STATE } from '@/domain/auth/auth-state';
import { isSafeReturnTo } from '@/domain/auth/return-to';

import type { AuthGateway } from './auth-gateway';

export type AuthStateListener = (state: AuthState) => void;

export class InvalidReturnToError extends Error {
  constructor(value: string) {
    super(`Unsafe return path: ${value}`);
    this.name = 'InvalidReturnToError';
  }
}

/**
 * Use cases for SSO login state. Framework-agnostic: UI code subscribes to state changes
 * and performs the browser navigation itself.
 */
export class AuthService {
  private current: AuthState = UNKNOWN_AUTH_STATE;
  private readonly listeners = new Set<AuthStateListener>();

  constructor(private readonly gateway: AuthGateway) {}

  get state(): AuthState {
    return this.current;
  }

  subscribe(listener: AuthStateListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Checks the backend for an existing session. Any failure (for example the backend being
   * unreachable) is treated as anonymous, so the user can retry by signing in.
   */
  async restore(): Promise<void> {
    try {
      const user = await this.gateway.fetchCurrentUser();
      this.setState(user === null ? { status: 'anonymous' } : { status: 'authenticated', user });
    } catch {
      this.setState({ status: 'anonymous' });
    }
  }

  /** Returns the URL the browser must navigate to in order to start SSO login. */
  signIn(returnTo: string): string {
    if (!isSafeReturnTo(returnTo)) {
      throw new InvalidReturnToError(returnTo);
    }
    return this.gateway.loginUrl(returnTo);
  }

  /** Ends the session. Local state becomes anonymous even if the request fails. */
  async signOut(): Promise<void> {
    try {
      await this.gateway.logout();
    } finally {
      this.setState({ status: 'anonymous' });
    }
  }

  private setState(next: AuthState): void {
    this.current = next;
    for (const listener of this.listeners) {
      listener(next);
    }
  }
}
