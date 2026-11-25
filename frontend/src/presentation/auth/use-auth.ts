import { computed, inject, onScopeDispose, shallowRef, type ComputedRef } from 'vue';

import type { AuthState } from '@/domain/auth/auth-state';

import { AUTH_SERVICE_KEY } from './auth-injection';

export interface UseAuth {
  readonly state: ComputedRef<AuthState>;
  /** Starts SSO login. The browser leaves the app; `returnTo` must be a safe relative path. */
  signIn(returnTo: string): void;
  signOut(): Promise<void>;
}

/** Reactive view of the auth state for components. Must run inside `setup()`. */
export function useAuth(): UseAuth {
  const service = inject(AUTH_SERVICE_KEY);
  if (service === undefined) {
    throw new Error('AuthService is not provided. Call app.provide(AUTH_SERVICE_KEY, ...).');
  }

  const current = shallowRef(service.state);
  onScopeDispose(
    service.subscribe((next) => {
      current.value = next;
    }),
  );

  return {
    state: computed(() => current.value),
    signIn(returnTo: string): void {
      window.location.assign(service.signIn(returnTo));
    },
    signOut(): Promise<void> {
      return service.signOut();
    },
  };
}
