import type { User } from '@/domain/auth/user';

/**
 * Port: how the application talks to the authentication backend.
 * Infrastructure provides the implementation; the application never sees HTTP.
 */
export interface AuthGateway {
  /** The signed-in user, or `null` when there is no valid session. */
  fetchCurrentUser(): Promise<User | null>;
  /** URL that starts the SSO login and returns the user to `returnTo` afterwards. */
  loginUrl(returnTo: string): string;
  /** Ends the current session on the backend. */
  logout(): Promise<void>;
}
