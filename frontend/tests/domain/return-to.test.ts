import { describe, expect, it } from 'vitest';

import { isSafeReturnTo, toSafeReturnTo } from '@/domain/auth/return-to';

describe('isSafeReturnTo', () => {
  it.each(['/', '/projects/1', '/projects?tab=logs', '/a/b#c'])('accepts %s', (value) => {
    expect(isSafeReturnTo(value)).toBe(true);
  });

  it.each([
    ['empty', ''],
    ['no leading slash', 'projects'],
    ['absolute URL', 'https://evil.example'],
    ['protocol-relative', '//evil.example'],
    ['backslash variant', '/\\evil.example'],
    ['CR/LF injection', '/ok\r\nSet-Cookie: x=1'],
    ['NUL byte', '/a\u0000b'],
    ['DEL character', '/a\u007fb'],
  ])('rejects %s', (_label, value) => {
    expect(isSafeReturnTo(value)).toBe(false);
  });

  it('rejects paths longer than 2048 characters', () => {
    expect(isSafeReturnTo(`/${'a'.repeat(2048)}`)).toBe(false);
    expect(isSafeReturnTo(`/${'a'.repeat(2047)}`)).toBe(true);
  });
});

describe('toSafeReturnTo', () => {
  it('returns the value when it is safe', () => {
    expect(toSafeReturnTo('/projects/1')).toBe('/projects/1');
  });

  it.each([undefined, null, 42, ['/a'], 'https://evil.example'])(
    'falls back to / for %j',
    (value: unknown) => {
      expect(toSafeReturnTo(value)).toBe('/');
    },
  );
});
