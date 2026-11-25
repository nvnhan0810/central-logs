import { describe, expect, it } from 'vitest';

import { displayName, type User } from '@/domain/auth/user';

const base: User = { subject: 'sub-1', email: null, name: null };

describe('displayName', () => {
  it('prefers the name', () => {
    expect(displayName({ ...base, name: ' Nguyen Van A ', email: 'a@example.com' })).toBe(
      'Nguyen Van A',
    );
  });

  it('falls back to the email when the name is missing or blank', () => {
    expect(displayName({ ...base, name: '   ', email: 'a@example.com' })).toBe('a@example.com');
  });

  it('falls back to the subject when neither name nor email is set', () => {
    expect(displayName(base)).toBe('sub-1');
  });
});
