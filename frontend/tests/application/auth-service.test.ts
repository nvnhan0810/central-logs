import { describe, expect, it, vi, type Mock } from 'vitest';

import type { AuthGateway } from '@/application/auth/auth-gateway';
import { AuthService, InvalidReturnToError } from '@/application/auth/auth-service';
import type { AuthState } from '@/domain/auth/auth-state';
import type { User } from '@/domain/auth/user';

const user: User = { subject: 'sub-1', email: 'a@example.com', name: 'A' };

interface FakeGateway {
  readonly gateway: AuthGateway;
  readonly fetchCurrentUser: Mock<() => Promise<User | null>>;
  readonly loginUrl: Mock<(returnTo: string) => string>;
  readonly logout: Mock<() => Promise<void>>;
}

function fakeGateway(
  overrides: Partial<Pick<FakeGateway, 'fetchCurrentUser' | 'logout'>> = {},
): FakeGateway {
  const fetchCurrentUser =
    overrides.fetchCurrentUser ?? vi.fn<() => Promise<User | null>>(() => Promise.resolve(null));
  const loginUrl = vi.fn((returnTo: string) => `/api/auth/login?return_to=${returnTo}`);
  const logout = overrides.logout ?? vi.fn<() => Promise<void>>(() => Promise.resolve());
  return {
    gateway: { fetchCurrentUser: () => fetchCurrentUser(), loginUrl, logout: () => logout() },
    fetchCurrentUser,
    loginUrl,
    logout,
  };
}

describe('AuthService', () => {
  it('starts in the unknown state', () => {
    expect(new AuthService(fakeGateway().gateway).state).toEqual({ status: 'unknown' });
  });

  it('restore() becomes authenticated when the backend knows the user', async () => {
    const gateway = fakeGateway({ fetchCurrentUser: vi.fn(() => Promise.resolve(user)) });
    const service = new AuthService(gateway.gateway);

    await service.restore();

    expect(service.state).toEqual({ status: 'authenticated', user });
  });

  it('restore() becomes anonymous when there is no session', async () => {
    const service = new AuthService(fakeGateway().gateway);

    await service.restore();

    expect(service.state).toEqual({ status: 'anonymous' });
  });

  it('restore() becomes anonymous when the backend cannot be reached', async () => {
    const gateway = fakeGateway({
      fetchCurrentUser: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const service = new AuthService(gateway.gateway);

    await service.restore();

    expect(service.state).toEqual({ status: 'anonymous' });
  });

  it('notifies subscribers on every change and stops after unsubscribe', async () => {
    const gateway = fakeGateway({ fetchCurrentUser: vi.fn(() => Promise.resolve(user)) });
    const service = new AuthService(gateway.gateway);
    const seen: AuthState[] = [];
    const unsubscribe = service.subscribe((state) => seen.push(state));

    await service.restore();
    unsubscribe();
    await service.signOut();

    expect(seen).toEqual([{ status: 'authenticated', user }]);
  });

  it('signIn() returns the login URL for a safe return path', () => {
    const gateway = fakeGateway();

    const url = new AuthService(gateway.gateway).signIn('/projects/1');

    expect(url).toBe('/api/auth/login?return_to=/projects/1');
    expect(gateway.loginUrl).toHaveBeenCalledWith('/projects/1');
  });

  it('signIn() refuses an unsafe return path before calling the gateway', () => {
    const gateway = fakeGateway();

    expect(() => new AuthService(gateway.gateway).signIn('https://evil.example')).toThrow(
      InvalidReturnToError,
    );
    expect(gateway.loginUrl).not.toHaveBeenCalled();
  });

  it('signOut() calls the backend and becomes anonymous', async () => {
    const gateway = fakeGateway({ fetchCurrentUser: vi.fn(() => Promise.resolve(user)) });
    const service = new AuthService(gateway.gateway);
    await service.restore();

    await service.signOut();

    expect(gateway.logout).toHaveBeenCalledOnce();
    expect(service.state).toEqual({ status: 'anonymous' });
  });

  it('signOut() still becomes anonymous when the request fails, and rethrows', async () => {
    const gateway = fakeGateway({
      fetchCurrentUser: vi.fn(() => Promise.resolve(user)),
      logout: vi.fn(() => Promise.reject(new Error('502'))),
    });
    const service = new AuthService(gateway.gateway);
    await service.restore();

    await expect(service.signOut()).rejects.toThrow('502');
    expect(service.state).toEqual({ status: 'anonymous' });
  });
});
