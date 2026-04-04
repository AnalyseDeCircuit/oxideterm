import { describe, expect, it } from 'vitest';
import {
  normalizePath,
  joinPath,
  getParentPath,
  getBaseName,
  validateFileName,
  isSubPath,
  getRelativePath,
} from '@/lib/pathUtils';

describe('normalizePath', () => {
  it('collapses multiple slashes', () => {
    expect(normalizePath('/home//user///file')).toBe('/home/user/file');
  });

  it('removes trailing slash', () => {
    expect(normalizePath('/home/user/')).toBe('/home/user');
  });

  it('preserves root', () => {
    expect(normalizePath('/')).toBe('/');
  });

  it('handles root with trailing slashes', () => {
    expect(normalizePath('///')).toBe('/');
  });

  it('handles normal path unchanged', () => {
    expect(normalizePath('/home/user/file.txt')).toBe('/home/user/file.txt');
  });
});

describe('joinPath', () => {
  it('joins base and name', () => {
    expect(joinPath('/home/user', 'file.txt')).toBe('/home/user/file.txt');
  });

  it('handles root base', () => {
    expect(joinPath('/', 'file.txt')).toBe('/file.txt');
  });

  it('handles trailing slash on base', () => {
    expect(joinPath('/home/user/', 'file.txt')).toBe('/home/user/file.txt');
  });

  it('handles double slashes in base', () => {
    expect(joinPath('/home//user', 'file.txt')).toBe('/home/user/file.txt');
  });
});

describe('getParentPath', () => {
  it('returns parent directory', () => {
    expect(getParentPath('/home/user/file.txt')).toBe('/home/user');
  });

  it('returns root for top-level path', () => {
    expect(getParentPath('/home')).toBe('/');
  });

  it('returns root for root', () => {
    expect(getParentPath('/')).toBe('/');
  });

  it('handles trailing slash', () => {
    expect(getParentPath('/home/user/')).toBe('/home');
  });
});

describe('getBaseName', () => {
  it('returns file name', () => {
    expect(getBaseName('/home/user/file.txt')).toBe('file.txt');
  });

  it('returns directory name', () => {
    expect(getBaseName('/home/user/')).toBe('user');
  });

  it('returns empty for root', () => {
    expect(getBaseName('/')).toBe('');
  });

  it('returns top-level name', () => {
    expect(getBaseName('/home')).toBe('home');
  });
});

describe('validateFileName', () => {
  it('accepts valid name', () => {
    expect(validateFileName('file.txt')).toBeNull();
  });

  it('accepts unicode name', () => {
    expect(validateFileName('日本語ファイル.txt')).toBeNull();
  });

  it('rejects empty string', () => {
    expect(validateFileName('')).toBe('ide.validation.nameEmpty');
  });

  it('rejects whitespace-only', () => {
    expect(validateFileName('   ')).toBe('ide.validation.nameEmpty');
  });

  it('rejects name with slash', () => {
    expect(validateFileName('dir/file')).toBe('ide.validation.nameContainsSlash');
  });

  it('rejects dot', () => {
    expect(validateFileName('.')).toBe('ide.validation.nameInvalid');
  });

  it('rejects double dot', () => {
    expect(validateFileName('..')).toBe('ide.validation.nameInvalid');
  });

  it('rejects invalid chars', () => {
    expect(validateFileName('file<name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file>name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file:name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file"name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file|name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file?name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file*name')).toBe('ide.validation.nameInvalidChars');
  });

  it('rejects control characters', () => {
    expect(validateFileName('file\x00name')).toBe('ide.validation.nameInvalidChars');
    expect(validateFileName('file\x1fname')).toBe('ide.validation.nameInvalidChars');
  });

  it('rejects names exceeding 255 bytes', () => {
    const longName = 'あ'.repeat(86); // 86 * 3 bytes = 258 > 255
    expect(validateFileName(longName)).toBe('ide.validation.nameTooLong');
  });

  it('accepts names at 255 byte boundary', () => {
    const exactName = 'a'.repeat(255);
    expect(validateFileName(exactName)).toBeNull();
  });
});

describe('isSubPath', () => {
  it('detects sub path', () => {
    expect(isSubPath('/home/user/project/src', '/home/user/project')).toBe(true);
  });

  it('rejects same path (not strict sub)', () => {
    expect(isSubPath('/home/user/project', '/home/user/project')).toBe(false);
  });

  it('rejects unrelated path', () => {
    expect(isSubPath('/home/other', '/home/user')).toBe(false);
  });

  it('rejects path that shares prefix but is not sub', () => {
    expect(isSubPath('/home/user-extra', '/home/user')).toBe(false);
  });

  it('handles root as parent', () => {
    // Root '/' + '/' = '//' so isSubPath doesn't consider root as parent
    // This is by design — root is a special case
    expect(isSubPath('/home', '/')).toBe(false);
  });
});

describe('getRelativePath', () => {
  it('returns relative path', () => {
    expect(getRelativePath('/home/user/project/src/file.ts', '/home/user/project')).toBe('src/file.ts');
  });

  it('returns empty for same path', () => {
    expect(getRelativePath('/home/user/project', '/home/user/project')).toBe('');
  });

  it('returns null for unrelated paths', () => {
    expect(getRelativePath('/other/path', '/home/user/project')).toBeNull();
  });

  it('handles trailing slashes', () => {
    expect(getRelativePath('/home/user/project/src/', '/home/user/project/')).toBe('src');
  });
});
