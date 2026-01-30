/**
 * 路径处理工具函数
 * 用于 IDE 模式的文件操作，统一处理各种边界情况
 */

/**
 * 规范化路径（处理双斜杠、末尾斜杠等）
 * @example
 * normalizePath("/home/user//file") // → "/home/user/file"
 * normalizePath("/home/user/") // → "/home/user"
 * normalizePath("/") // → "/"
 */
export function normalizePath(path: string): string {
  return (
    path
      .replace(/\/+/g, '/') // 多个斜杠变单斜杠
      .replace(/\/$/, '') || '/'
  ); // 移除末尾斜杠，但保留根目录
}

/**
 * 拼接路径（安全处理各种边界情况）
 * @example
 * joinPath("/home/user", "file.txt") // → "/home/user/file.txt"
 * joinPath("/", "file.txt") // → "/file.txt"
 * joinPath("/home/user/", "file.txt") // → "/home/user/file.txt"
 */
export function joinPath(base: string, name: string): string {
  const normalizedBase = normalizePath(base);
  if (normalizedBase === '/') {
    return `/${name}`;
  }
  return `${normalizedBase}/${name}`;
}

/**
 * 获取父目录路径
 * @example
 * getParentPath("/home/user/file.txt") // → "/home/user"
 * getParentPath("/home") // → "/"
 * getParentPath("/") // → "/"
 */
export function getParentPath(path: string): string {
  const normalized = normalizePath(path);
  const lastSlash = normalized.lastIndexOf('/');
  if (lastSlash <= 0) return '/';
  return normalized.substring(0, lastSlash);
}

/**
 * 获取文件/目录名
 * @example
 * getBaseName("/home/user/file.txt") // → "file.txt"
 * getBaseName("/home/user/") // → "user"
 * getBaseName("/") // → ""
 */
export function getBaseName(path: string): string {
  const normalized = normalizePath(path);
  if (normalized === '/') return '';
  const lastSlash = normalized.lastIndexOf('/');
  return normalized.substring(lastSlash + 1);
}

/**
 * 验证文件名是否合法
 * @returns null 表示合法，否则返回错误信息键（用于 i18n）
 */
export function validateFileName(name: string): string | null {
  if (!name || !name.trim()) {
    return 'ide.validation.nameEmpty';
  }
  if (name.includes('/')) {
    return 'ide.validation.nameContainsSlash';
  }
  if (name === '.' || name === '..') {
    return 'ide.validation.nameInvalid';
  }
  // 检查其他非法字符（适用于大多数文件系统）
  const invalidChars = /[<>:"|?*\x00-\x1f]/;
  if (invalidChars.test(name)) {
    return 'ide.validation.nameInvalidChars';
  }
  // 文件名过长（大多数系统限制 255 字节）
  if (new TextEncoder().encode(name).length > 255) {
    return 'ide.validation.nameTooLong';
  }
  return null;
}

/**
 * 检查路径是否是另一个路径的子路径
 * @example
 * isSubPath("/home/user/project/src", "/home/user/project") // → true
 * isSubPath("/home/user/project", "/home/user/project") // → false (不是严格子路径)
 * isSubPath("/home/other", "/home/user") // → false
 */
export function isSubPath(childPath: string, parentPath: string): boolean {
  const normalizedChild = normalizePath(childPath);
  const normalizedParent = normalizePath(parentPath);

  // 必须以父路径 + "/" 开头才是子路径
  return normalizedChild.startsWith(normalizedParent + '/');
}

/**
 * 获取相对路径
 * @example
 * getRelativePath("/home/user/project/src/file.ts", "/home/user/project") // → "src/file.ts"
 * getRelativePath("/home/user/project", "/home/user/project") // → ""
 * getRelativePath("/other/path", "/home/user/project") // → null
 */
export function getRelativePath(
  fullPath: string,
  basePath: string
): string | null {
  const normalizedFull = normalizePath(fullPath);
  const normalizedBase = normalizePath(basePath);

  if (normalizedFull === normalizedBase) {
    return '';
  }

  if (normalizedFull.startsWith(normalizedBase + '/')) {
    return normalizedFull.substring(normalizedBase.length + 1);
  }

  return null;
}
