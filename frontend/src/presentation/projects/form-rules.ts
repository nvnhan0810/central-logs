/** Longest name the backend accepts (characters, after trimming). */
export const NAME_MAX_LENGTH = 100;

/** Returns the problem with a project or sub-project name, or `null` when it is usable. */
export function nameError(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === '') {
    return 'Tên không được để trống.';
  }
  if (Array.from(trimmed).length > NAME_MAX_LENGTH) {
    return `Tên dài tối đa ${String(NAME_MAX_LENGTH)} ký tự.`;
  }
  return null;
}

/** Splits the directory field into one path per non-blank line. */
export function parseDirectories(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== '');
}

/**
 * Checks the directory list before it is sent. The backend stays the authority (it also checks
 * that each directory exists); this only catches what the user can fix on the spot.
 */
export function directoriesError(directories: readonly string[]): string | null {
  if (directories.length === 0) {
    return 'Cần ít nhất một thư mục chứa log.';
  }
  const seen = new Set<string>();
  for (const dir of directories) {
    if (!isAbsolutePath(dir)) {
      return `Thư mục phải là đường dẫn tuyệt đối: ${dir}`;
    }
    if (dir.split(/[\\/]/).includes('..')) {
      return `Thư mục không được chứa "..": ${dir}`;
    }
    if (seen.has(dir)) {
      return `Thư mục bị trùng: ${dir}`;
    }
    seen.add(dir);
  }
  return null;
}

/** Unix absolute path (`/var/log`) or Windows drive path (`C:\logs`, `C:/logs`). */
function isAbsolutePath(path: string): boolean {
  return /^(\/|[A-Za-z]:[\\/])/.test(path);
}
