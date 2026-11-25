import type { User } from './user';

/**
 * Where the user stands with respect to the session.
 * `unknown` is only valid before the first check against the backend.
 */
export type AuthState =
  | { readonly status: 'unknown' }
  | { readonly status: 'anonymous' }
  | { readonly status: 'authenticated'; readonly user: User };

export const UNKNOWN_AUTH_STATE: AuthState = { status: 'unknown' };
