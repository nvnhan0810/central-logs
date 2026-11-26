// Runtime checks for the backend's JSON (snake_case, see backend/src/interfaces/http/dto.rs).
// A response that does not match fails loudly instead of rendering wrong data.

type Json = Record<string, unknown>;

function fail(what: string): never {
  throw new Error(`Unexpected response shape: ${what}`);
}

export function asArray(value: unknown, what: string): unknown[] {
  return Array.isArray(value) ? value : fail(what);
}

export function asObject(value: unknown, what: string): Json {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Json)
    : fail(what);
}

export function asString(value: unknown, what: string): string {
  return typeof value === 'string' ? value : fail(what);
}

export function asBoolean(value: unknown, what: string): boolean {
  return typeof value === 'boolean' ? value : fail(what);
}

export function asNumber(value: unknown, what: string): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fail(what);
}

export function asNullableNumber(value: unknown, what: string): number | null {
  return value === null ? null : asNumber(value, what);
}

export function asStringArray(value: unknown, what: string): string[] {
  return asArray(value, what).map((item) => asString(item, what));
}
