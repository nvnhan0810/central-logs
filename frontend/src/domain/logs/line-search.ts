/** How the search box reads the query: plain text, or a JavaScript regular expression. */
export type SearchMode = 'text' | 'regex';

export type LineMatcher =
  | { readonly ok: true; readonly test: (line: string) => boolean }
  | { readonly ok: false; readonly error: string };

/**
 * Builds the predicate for the search box.
 * - An empty query matches every line.
 * - `text` is a case-insensitive substring match.
 * - `regex` is a case-insensitive RegExp. An invalid pattern is reported, not thrown.
 */
export function createLineMatcher(query: string, mode: SearchMode): LineMatcher {
  if (query === '') {
    return { ok: true, test: () => true };
  }
  if (mode === 'text') {
    const needle = query.toLowerCase();
    return { ok: true, test: (line) => line.toLowerCase().includes(needle) };
  }
  try {
    // No `g` flag: `test` must not keep state between lines.
    const pattern = new RegExp(query, 'i');
    return { ok: true, test: (line) => pattern.test(line) };
  } catch (error: unknown) {
    const reason = error instanceof SyntaxError ? error.message : 'invalid pattern';
    return { ok: false, error: `Regex không hợp lệ: ${reason}` };
  }
}
