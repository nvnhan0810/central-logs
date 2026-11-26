import { describe, expect, it } from 'vitest';

import {
  NAME_MAX_LENGTH,
  directoriesError,
  nameError,
  parseDirectories,
} from '@/presentation/projects/form-rules';

describe('nameError', () => {
  it('rejects a blank name after trimming', () => {
    expect(nameError('   ')).toBe('Tên không được để trống.');
  });

  it('accepts a name up to the limit, counting characters', () => {
    expect(nameError('a'.repeat(NAME_MAX_LENGTH))).toBeNull();
    expect(nameError('ă'.repeat(NAME_MAX_LENGTH))).toBeNull();
    expect(nameError('a'.repeat(NAME_MAX_LENGTH + 1))).not.toBeNull();
  });
});

describe('parseDirectories', () => {
  it('splits lines, trims them and drops blank ones', () => {
    expect(parseDirectories(' /var/log/api \r\n\n/srv/app/logs\n   \n')).toEqual([
      '/var/log/api',
      '/srv/app/logs',
    ]);
  });
});

describe('directoriesError', () => {
  it('requires at least one directory', () => {
    expect(directoriesError([])).toBe('Cần ít nhất một thư mục chứa log.');
  });

  it('requires absolute paths, Unix or Windows', () => {
    expect(directoriesError(['/var/log'])).toBeNull();
    expect(directoriesError(['C:\\logs'])).toBeNull();
    expect(directoriesError(['D:/logs'])).toBeNull();
    expect(directoriesError(['logs/api'])).toBe('Thư mục phải là đường dẫn tuyệt đối: logs/api');
  });

  it('rejects parent references and duplicates', () => {
    expect(directoriesError(['/var/log/../etc'])).toBe(
      'Thư mục không được chứa "..": /var/log/../etc',
    );
    expect(directoriesError(['/var/log', '/var/log'])).toBe('Thư mục bị trùng: /var/log');
  });
});
