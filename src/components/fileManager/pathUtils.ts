// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type LocalPathStyle = 'windows' | 'posix';

const WINDOWS_DRIVE_RE = /^[A-Za-z]:(?:[\\/]|$)/;
const WINDOWS_UNC_RE = /^[\\/]{2}[^\\/]+[\\/][^\\/]+/;

export function detectLocalPathStyle(path: string): LocalPathStyle {
  if (WINDOWS_DRIVE_RE.test(path) || WINDOWS_UNC_RE.test(path) || path.startsWith('\\\\')) {
    return 'windows';
  }
  return 'posix';
}

export function isWindowsDriveRoot(path: string): boolean {
  return /^[A-Za-z]:\\$/.test(normalizeLocalPath(path, 'windows'));
}

export function isWindowsUncRoot(path: string): boolean {
  const normalized = normalizeLocalPath(path, 'windows');
  const root = getWindowsUncRoot(normalized);
  return root !== null && root === normalized;
}

function getWindowsUncRoot(path: string): string | null {
  const match = path.match(/^\\\\[^\\]+\\[^\\]+/);
  return match ? match[0] : null;
}

export function normalizeLocalPath(path: string, forcedStyle?: LocalPathStyle): string {
  const trimmed = path.trim();
  if (!trimmed) return '';

  const style = forcedStyle ?? detectLocalPathStyle(trimmed);
  if (style === 'windows') {
    if (trimmed.startsWith('\\\\') || WINDOWS_UNC_RE.test(trimmed)) {
      const parts = trimmed.replace(/^[\\/]+/, '').split(/[\\/]+/).filter(Boolean);
      if (parts.length < 2) {
        return parts.length === 0 ? '\\\\' : `\\\\${parts.join('\\')}`;
      }
      const [server, share, ...rest] = parts;
      const root = `\\\\${server}\\${share}`;
      return rest.length > 0 ? `${root}\\${rest.join('\\')}` : root;
    }

    const driveMatch = trimmed.match(/^([A-Za-z]):(?:[\\/]+)?(.*)$/);
    if (driveMatch) {
      const drive = `${driveMatch[1].toUpperCase()}:`;
      const parts = driveMatch[2].split(/[\\/]+/).filter(Boolean);
      if (parts.length === 0) return `${drive}\\`;
      return `${drive}\\${parts.join('\\')}`;
    }

    return trimmed.split(/[\\/]+/).filter(Boolean).join('\\');
  }

  const collapsed = trimmed.replace(/\/+/g, '/');
  if (collapsed === '/') return '/';
  return collapsed.replace(/\/+$/g, '') || '/';
}

export function joinLocalPath(base: string, name: string, forcedStyle?: LocalPathStyle): string {
  const style = forcedStyle ?? detectLocalPathStyle(base);
  const normalizedBase = normalizeLocalPath(base, style);
  if (!normalizedBase) return name;

  if (style === 'windows') {
    if (isWindowsDriveRoot(normalizedBase)) {
      return `${normalizedBase}${name}`;
    }
    return `${normalizedBase}\\${name}`;
  }

  if (normalizedBase === '/') {
    return `/${name}`;
  }
  return `${normalizedBase}/${name}`;
}

export function getLocalParentPath(path: string, forcedStyle?: LocalPathStyle): string | '__DRIVES__' {
  const style = forcedStyle ?? detectLocalPathStyle(path);
  const normalized = normalizeLocalPath(path, style);
  if (!normalized) return '';

  if (style === 'windows') {
    if (isWindowsDriveRoot(normalized)) {
      return '__DRIVES__';
    }

    if (isWindowsUncRoot(normalized)) {
      return normalized;
    }

    const separatorIndex = normalized.lastIndexOf('\\');
    if (separatorIndex <= 0) return normalized;

    const parent = normalized.slice(0, separatorIndex);
    if (/^[A-Za-z]:$/.test(parent)) {
      return `${parent}\\`;
    }

    const uncRoot = getWindowsUncRoot(normalized);
    if (uncRoot && parent.length < uncRoot.length) {
      return uncRoot;
    }

    return parent || normalized;
  }

  if (normalized === '/') return '/';
  const separatorIndex = normalized.lastIndexOf('/');
  if (separatorIndex <= 0) return '/';
  return normalized.slice(0, separatorIndex);
}

export function getLocalBaseName(path: string, forcedStyle?: LocalPathStyle): string {
  const style = forcedStyle ?? detectLocalPathStyle(path);
  const normalized = normalizeLocalPath(path, style);
  if (!normalized) return '';

  if (style === 'windows') {
    if (isWindowsDriveRoot(normalized) || isWindowsUncRoot(normalized)) {
      return normalized;
    }
    const separatorIndex = normalized.lastIndexOf('\\');
    return separatorIndex === -1 ? normalized : normalized.slice(separatorIndex + 1);
  }

  if (normalized === '/') return '';
  const separatorIndex = normalized.lastIndexOf('/');
  return separatorIndex === -1 ? normalized : normalized.slice(separatorIndex + 1);
}

export function isLocalPathEqual(a: string, b: string, forcedStyle?: LocalPathStyle): boolean {
  const style = forcedStyle ?? detectLocalPathStyle(a || b);
  const left = normalizeLocalPath(a, style);
  const right = normalizeLocalPath(b, style);
  if (style === 'windows') {
    return left.toLowerCase() === right.toLowerCase();
  }
  return left === right;
}

export function isLocalPathWithin(parent: string, candidate: string, forcedStyle?: LocalPathStyle): boolean {
  const style = forcedStyle ?? detectLocalPathStyle(parent || candidate);
  const normalizedParent = normalizeLocalPath(parent, style);
  const normalizedCandidate = normalizeLocalPath(candidate, style);

  if (!normalizedParent || !normalizedCandidate) {
    return false;
  }

  if (isLocalPathEqual(normalizedParent, normalizedCandidate, style)) {
    return true;
  }

  const separator = style === 'windows' ? '\\' : '/';
  const parentPrefix = normalizedParent.endsWith(separator)
    ? normalizedParent
    : `${normalizedParent}${separator}`;

  if (style === 'windows') {
    return normalizedCandidate.toLowerCase().startsWith(parentPrefix.toLowerCase());
  }
  return normalizedCandidate.startsWith(parentPrefix);
}

export function validateLocalFileName(name: string): string | null {
  const trimmed = name.trim();
  if (!trimmed) {
    return 'ide.validation.nameEmpty';
  }
  if (trimmed.includes('/') || trimmed.includes('\\')) {
    return 'ide.validation.nameContainsSlash';
  }
  if (trimmed === '.' || trimmed === '..') {
    return 'ide.validation.nameInvalid';
  }
  if (/[<>:"|?*\x00-\x1f]/.test(trimmed)) {
    return 'ide.validation.nameInvalidChars';
  }
  if (new TextEncoder().encode(trimmed).length > 255) {
    return 'ide.validation.nameTooLong';
  }
  return null;
}