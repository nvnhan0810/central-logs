import { describe, expect, it } from 'vitest';

import type { AuthState } from '@/domain/auth/auth-state';
import { decideAccess } from '@/presentation/router/access';

const anonymous: AuthState = { status: 'anonymous' };
const signedIn: AuthState = {
  status: 'authenticated',
  user: { subject: 'sub-1', email: null, name: 'A' },
};
const PROTECTED = { requiresAuth: true, guestOnly: false };
const GUEST = { requiresAuth: false, guestOnly: true };
const PUBLIC = { requiresAuth: false, guestOnly: false };

describe('decideAccess', () => {
  it('sends anonymous users from protected routes to login', () => {
    expect(decideAccess(anonymous, PROTECTED)).toBe('to-login');
  });

  it('lets signed-in users into protected routes', () => {
    expect(decideAccess(signedIn, PROTECTED)).toBe('allow');
  });

  it('sends signed-in users away from the login page', () => {
    expect(decideAccess(signedIn, GUEST)).toBe('to-home');
  });

  it('lets anonymous users see the login page and public routes', () => {
    expect(decideAccess(anonymous, GUEST)).toBe('allow');
    expect(decideAccess(anonymous, PUBLIC)).toBe('allow');
  });
});
