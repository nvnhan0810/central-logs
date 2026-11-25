/**
 * Where the user goes after login. Only same-site relative paths are allowed, which
 * mirrors the backend rule (backend/src/application/auth.rs `validate_return_to`) so that
 * login cannot be turned into an open redirect.
 */
export const DEFAULT_RETURN_TO = '/';

const MAX_RETURN_TO_LENGTH = 2048;

export function isSafeReturnTo(value: string): boolean {
  if (value.length > MAX_RETURN_TO_LENGTH) {
    return false;
  }
  if (!value.startsWith('/') || value.startsWith('//') || value.startsWith('/\\')) {
    return false;
  }
  // Rejects control characters (C0, DEL and C1), e.g. CR/LF used for header injection.
  return !/\p{Cc}/u.test(value);
}

/** Returns `value` when it is a safe return path, otherwise the default. */
export function toSafeReturnTo(value: unknown): string {
  return typeof value === 'string' && isSafeReturnTo(value) ? value : DEFAULT_RETURN_TO;
}
