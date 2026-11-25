import type { AuthState } from '@/domain/auth/auth-state';

export interface AccessRequirements {
  /** Route is only for signed-in users. */
  readonly requiresAuth: boolean;
  /** Route is only for anonymous users (the login page). */
  readonly guestOnly: boolean;
}

export type AccessDecision = 'allow' | 'to-login' | 'to-home';

/** Pure access rule, kept out of the router so it can be tested without a browser. */
export function decideAccess(state: AuthState, requirements: AccessRequirements): AccessDecision {
  // `unknown` is resolved by the caller before this runs; treat it as not signed in.
  const signedIn = state.status === 'authenticated';
  if (requirements.requiresAuth && !signedIn) {
    return 'to-login';
  }
  if (requirements.guestOnly && signedIn) {
    return 'to-home';
  }
  return 'allow';
}
