import { describe, expect, it } from 'vitest';

import { createLineMatcher } from '@/domain/logs/line-search';

function matchesOf(query: string, mode: 'text' | 'regex', lines: readonly string[]): string[] {
  const matcher = createLineMatcher(query, mode);
  if (!matcher.ok) {
    throw new Error(matcher.error);
  }
  return lines.filter((line) => matcher.test(line));
}

const LINES = [
  'INFO started worker-1',
  'ERROR db timeout after 30s',
  'warn: retrying request 42',
  'error code=500 path=/api/x',
];

describe('createLineMatcher', () => {
  it('matches everything for an empty query', () => {
    expect(matchesOf('', 'text', LINES)).toHaveLength(LINES.length);
    expect(matchesOf('', 'regex', LINES)).toHaveLength(LINES.length);
  });

  it('matches plain text case-insensitively', () => {
    expect(matchesOf('error', 'text', LINES)).toEqual([LINES[1], LINES[3]]);
  });

  it('treats regex metacharacters literally in text mode', () => {
    expect(matchesOf('code=500 path=/api', 'text', LINES)).toEqual([LINES[3]]);
    expect(matchesOf('30s.', 'text', LINES)).toEqual([]);
    expect(matchesOf('a+', 'text', ['a+b', 'aab'])).toEqual(['a+b']);
  });

  it('supports regular expressions, case-insensitively', () => {
    expect(matchesOf('^(info|warn)', 'regex', LINES)).toEqual([LINES[0], LINES[2]]);
    expect(matchesOf('\\b\\d{3}\\b', 'regex', LINES)).toEqual([LINES[3]]);
    expect(matchesOf('worker-\\d', 'regex', LINES)).toEqual([LINES[0]]);
  });

  it('reports an invalid regex instead of throwing', () => {
    const matcher = createLineMatcher('(unclosed', 'regex');
    expect(matcher.ok).toBe(false);
    if (!matcher.ok) {
      expect(matcher.error).toContain('Regex không hợp lệ');
    }
  });

  it('gives the same answer on repeated calls (no shared regex state)', () => {
    const matcher = createLineMatcher('err', 'regex');
    expect(matcher.ok).toBe(true);
    if (matcher.ok) {
      expect([matcher.test('error'), matcher.test('error'), matcher.test('error')]).toEqual([
        true,
        true,
        true,
      ]);
    }
  });
});
