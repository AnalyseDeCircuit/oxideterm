// src/lib/fileIcons.tsx
// 统一的文件图标工具，基于 lucide-react
// 用于 IDE 文件树、编辑器标签页、SFTP 视图等

import {
  File,
  FileCode,
  FileCode2,
  FileJson,
  FileJson2,
  FileText,
  FileImage,
  FileCog,
  FileTerminal,
  FileArchive,
  FileLock,
  FileVideo,
  FileAudio,
  FileSpreadsheet,
  Folder,
  FolderOpen,
  FolderGit,
  FolderGit2,
  type LucideIcon,
} from 'lucide-react';
import { cn } from './utils';

// ═══════════════════════════════════════════════════════════════════════════
// 扩展名到图标的映射
// ═══════════════════════════════════════════════════════════════════════════

const FILE_ICON_MAP: Record<string, LucideIcon> = {
  // TypeScript / JavaScript
  ts: FileCode2,
  tsx: FileCode2,
  mts: FileCode2,
  cts: FileCode2,
  js: FileCode,
  jsx: FileCode,
  mjs: FileCode,
  cjs: FileCode,

  // Rust
  rs: FileCode2,
  
  // Python
  py: FileCode,
  pyw: FileCode,
  pyi: FileCode,

  // Go
  go: FileCode,

  // Ruby
  rb: FileCode,
  rake: FileCode,

  // Java / Kotlin / Scala
  java: FileCode,
  kt: FileCode,
  kts: FileCode,
  scala: FileCode,

  // C / C++
  c: FileCode,
  h: FileCode,
  cpp: FileCode,
  cc: FileCode,
  cxx: FileCode,
  hpp: FileCode,
  hxx: FileCode,

  // Swift / Objective-C
  swift: FileCode,
  m: FileCode,
  mm: FileCode,

  // PHP
  php: FileCode,

  // Lua
  lua: FileCode,

  // R
  r: FileCode,
  rmd: FileCode,

  // SQL
  sql: FileCode,

  // Web frameworks
  vue: FileCode2,
  svelte: FileCode2,
  astro: FileCode2,

  // JSON
  json: FileJson2,
  jsonc: FileJson2,
  json5: FileJson,

  // Config files
  yaml: FileCog,
  yml: FileCog,
  toml: FileCog,
  ini: FileCog,
  conf: FileCog,
  cfg: FileCog,
  env: FileCog,
  envrc: FileCog,
  properties: FileCog,

  // Markdown / Text
  md: FileText,
  markdown: FileText,
  txt: FileText,
  text: FileText,
  rst: FileText,
  adoc: FileText,
  org: FileText,

  // HTML / XML
  html: FileCode,
  htm: FileCode,
  xhtml: FileCode,
  xml: FileCode,
  xsd: FileCode,
  xsl: FileCode,
  svg: FileImage,

  // CSS
  css: FileCode,
  scss: FileCode,
  sass: FileCode,
  less: FileCode,

  // Images
  png: FileImage,
  jpg: FileImage,
  jpeg: FileImage,
  gif: FileImage,
  webp: FileImage,
  ico: FileImage,
  bmp: FileImage,
  tiff: FileImage,
  tif: FileImage,

  // Video
  mp4: FileVideo,
  webm: FileVideo,
  mov: FileVideo,
  avi: FileVideo,
  mkv: FileVideo,
  flv: FileVideo,

  // Audio
  mp3: FileAudio,
  wav: FileAudio,
  ogg: FileAudio,
  flac: FileAudio,
  aac: FileAudio,
  m4a: FileAudio,

  // Shell / Terminal
  sh: FileTerminal,
  bash: FileTerminal,
  zsh: FileTerminal,
  fish: FileTerminal,
  ps1: FileTerminal,
  bat: FileTerminal,
  cmd: FileTerminal,

  // Archives
  zip: FileArchive,
  tar: FileArchive,
  gz: FileArchive,
  bz2: FileArchive,
  xz: FileArchive,
  '7z': FileArchive,
  rar: FileArchive,

  // Lock files
  lock: FileLock,

  // Spreadsheets
  csv: FileSpreadsheet,
  tsv: FileSpreadsheet,
  xls: FileSpreadsheet,
  xlsx: FileSpreadsheet,

  // PDF / Documents
  pdf: FileText,
  doc: FileText,
  docx: FileText,

  // Other
  diff: FileCode,
  patch: FileCode,
  log: FileText,
  tex: FileText,
  latex: FileText,
};

// ═══════════════════════════════════════════════════════════════════════════
// 特殊文件名映射
// ═══════════════════════════════════════════════════════════════════════════

const SPECIAL_FILE_MAP: Record<string, LucideIcon> = {
  dockerfile: FileTerminal,
  'docker-compose.yml': FileCog,
  'docker-compose.yaml': FileCog,
  '.dockerignore': FileCog,
  '.gitignore': FileCog,
  '.gitattributes': FileCog,
  '.gitmodules': FileCog,
  '.editorconfig': FileCog,
  '.prettierrc': FileCog,
  '.eslintrc': FileCog,
  '.eslintrc.json': FileCog,
  '.eslintrc.js': FileCog,
  'cargo.toml': FileCog,
  'cargo.lock': FileLock,
  'package.json': FileJson2,
  'package-lock.json': FileLock,
  'pnpm-lock.yaml': FileLock,
  'yarn.lock': FileLock,
  'tsconfig.json': FileJson2,
  'jsconfig.json': FileJson2,
  makefile: FileTerminal,
  'cmakelists.txt': FileTerminal,
  license: FileText,
  'license.md': FileText,
  'license.txt': FileText,
  readme: FileText,
  'readme.md': FileText,
  'readme.txt': FileText,
};

// ═══════════════════════════════════════════════════════════════════════════
// 颜色映射（使用项目主题色系）
// ═══════════════════════════════════════════════════════════════════════════

const FILE_COLORS: Record<string, string> = {
  // TypeScript - 蓝色
  ts: 'text-blue-400',
  tsx: 'text-blue-400',
  mts: 'text-blue-400',
  cts: 'text-blue-400',

  // JavaScript - 黄色
  js: 'text-yellow-400',
  jsx: 'text-yellow-400',
  mjs: 'text-yellow-400',
  cjs: 'text-yellow-400',

  // Rust - 橙色 (Oxide 主题色)
  rs: 'text-orange-500',

  // Python - 绿色
  py: 'text-green-400',
  pyw: 'text-green-400',
  pyi: 'text-green-400',

  // Go - 青色
  go: 'text-cyan-400',

  // Ruby - 红色
  rb: 'text-red-400',
  rake: 'text-red-400',

  // Java - 橙红色
  java: 'text-orange-400',
  kt: 'text-purple-400',
  scala: 'text-red-500',

  // C/C++ - 蓝色
  c: 'text-blue-500',
  h: 'text-blue-500',
  cpp: 'text-blue-500',
  hpp: 'text-blue-500',

  // JSON - 黄色
  json: 'text-yellow-500',
  jsonc: 'text-yellow-500',
  json5: 'text-yellow-500',

  // Config - 紫色
  yaml: 'text-purple-400',
  yml: 'text-purple-400',
  toml: 'text-orange-400',

  // Markdown - 灰蓝色
  md: 'text-slate-400',
  markdown: 'text-slate-400',

  // HTML - 橙红色
  html: 'text-orange-500',
  htm: 'text-orange-500',

  // CSS - 蓝紫色
  css: 'text-blue-500',
  scss: 'text-pink-400',
  sass: 'text-pink-400',

  // Vue - 绿色
  vue: 'text-emerald-400',

  // Svelte - 橙色
  svelte: 'text-orange-500',

  // Images - 紫色
  png: 'text-purple-400',
  jpg: 'text-purple-400',
  jpeg: 'text-purple-400',
  gif: 'text-purple-400',
  svg: 'text-yellow-400',

  // Shell - 绿色
  sh: 'text-green-500',
  bash: 'text-green-500',
  zsh: 'text-green-500',

  // Lock - 灰色
  lock: 'text-zinc-500',
};

// 特殊文件名的颜色
const SPECIAL_FILE_COLORS: Record<string, string> = {
  dockerfile: 'text-blue-400',
  '.gitignore': 'text-orange-400',
  'cargo.toml': 'text-orange-500',
  'package.json': 'text-green-400',
  'tsconfig.json': 'text-blue-400',
};

// 默认图标颜色 - 使用 CSS 变量以支持主题切换
const DEFAULT_FILE_COLOR = 'var(--theme-text-muted)';
const DEFAULT_FOLDER_COLOR = 'var(--theme-accent)';
const GIT_FOLDER_COLOR = 'var(--theme-accent)';

// 判断颜色是否为 CSS 变量
const isThemeVariable = (color: string) => color.startsWith('var(');

// ═══════════════════════════════════════════════════════════════════════════
// 导出函数
// ═══════════════════════════════════════════════════════════════════════════

export interface FileIconInfo {
  Icon: LucideIcon;
  color: string;
}

/**
 * 获取文件图标和颜色
 * @param filename 文件名
 * @param overrideColor 可选的覆盖颜色（如 Git 状态颜色）
 */
export function getFileIcon(filename: string, overrideColor?: string): FileIconInfo {
  const lowerName = filename.toLowerCase();

  // 检查特殊文件名
  if (SPECIAL_FILE_MAP[lowerName]) {
    return {
      Icon: SPECIAL_FILE_MAP[lowerName],
      color: overrideColor || SPECIAL_FILE_COLORS[lowerName] || DEFAULT_FILE_COLOR,
    };
  }

  // 获取扩展名
  const ext = filename.includes('.')
    ? filename.split('.').pop()?.toLowerCase() || ''
    : '';

  return {
    Icon: FILE_ICON_MAP[ext] || File,
    color: overrideColor || FILE_COLORS[ext] || DEFAULT_FILE_COLOR,
  };
}

/**
 * 获取文件夹图标
 */
export function getFolderIcon(isOpen: boolean, isGit?: boolean): LucideIcon {
  if (isGit) return isOpen ? FolderGit2 : FolderGit;
  return isOpen ? FolderOpen : Folder;
}

/**
 * 获取文件夹颜色
 */
export function getFolderColor(isGit?: boolean): string {
  return isGit ? GIT_FOLDER_COLOR : DEFAULT_FOLDER_COLOR;
}

// ═══════════════════════════════════════════════════════════════════════════
// React 组件
// ═══════════════════════════════════════════════════════════════════════════

interface FileIconProps {
  filename: string;
  className?: string;
  size?: number;
  /** 可选的覆盖颜色（如 Git 状态颜色），优先于默认颜色 */
  overrideColor?: string;
}

/**
 * 文件图标组件
 * @example
 * // 普通文件
 * <FileIcon filename="main.ts" />
 * 
 * // 带 Git 状态颜色
 * <FileIcon filename="main.ts" overrideColor="text-yellow-500" />
 */
export function FileIcon({ filename, className, size = 14, overrideColor }: FileIconProps) {
  const { Icon, color } = getFileIcon(filename, overrideColor);
  
  // 如果颜色是 CSS 变量，使用内联样式；否则使用 Tailwind 类名
  if (isThemeVariable(color)) {
    return <Icon className={className} style={{ color }} size={size} />;
  }
  return <Icon className={cn(color, className)} size={size} />;
}

interface FolderIconProps {
  isOpen?: boolean;
  isGit?: boolean;
  className?: string;
  size?: number;
}

/**
 * 文件夹图标组件
 */
export function FolderIcon({ isOpen = false, isGit = false, className, size = 14 }: FolderIconProps) {
  const Icon = getFolderIcon(isOpen, isGit);
  const color = getFolderColor(isGit);
  
  // 文件夹颜色使用主题变量，通过内联样式应用
  if (isThemeVariable(color)) {
    return <Icon className={className} style={{ color }} size={size} />;
  }
  return <Icon className={cn(color, className)} size={size} />;
}

// ═══════════════════════════════════════════════════════════════════════════
// 语言到图标映射（用于编辑器标签页）
// ═══════════════════════════════════════════════════════════════════════════

const LANGUAGE_ICON_MAP: Record<string, LucideIcon> = {
  typescript: FileCode2,
  javascript: FileCode,
  rust: FileCode2,
  python: FileCode,
  go: FileCode,
  ruby: FileCode,
  java: FileCode,
  kotlin: FileCode,
  scala: FileCode,
  c: FileCode,
  cpp: FileCode,
  swift: FileCode,
  php: FileCode,
  lua: FileCode,
  r: FileCode,
  sql: FileCode,
  json: FileJson2,
  yaml: FileCog,
  toml: FileCog,
  markdown: FileText,
  html: FileCode,
  css: FileCode,
  shell: FileTerminal,
  plaintext: FileText,
};

const LANGUAGE_COLOR_MAP: Record<string, string> = {
  typescript: 'text-blue-400',
  javascript: 'text-yellow-400',
  rust: 'text-orange-500',
  python: 'text-green-400',
  go: 'text-cyan-400',
  ruby: 'text-red-400',
  java: 'text-orange-400',
  kotlin: 'text-purple-400',
  json: 'text-yellow-500',
  yaml: 'text-purple-400',
  toml: 'text-orange-400',
  markdown: 'text-slate-400',
  html: 'text-orange-500',
  css: 'text-blue-500',
  shell: 'text-green-500',
  plaintext: 'var(--theme-text-muted)',
};

/**
 * 根据语言获取图标和颜色（用于编辑器标签页）
 */
export function getLanguageIcon(language: string): FileIconInfo {
  const lowerLang = language.toLowerCase();
  return {
    Icon: LANGUAGE_ICON_MAP[lowerLang] || FileCode,
    color: LANGUAGE_COLOR_MAP[lowerLang] || DEFAULT_FILE_COLOR,
  };
}

interface LanguageIconProps {
  language: string;
  className?: string;
  size?: number;
}

/**
 * 语言图标组件（用于编辑器标签页）
 */
export function LanguageIcon({ language, className, size = 14 }: LanguageIconProps) {
  const { Icon, color } = getLanguageIcon(language);
  
  // 如果颜色是 CSS 变量，使用内联样式
  if (isThemeVariable(color)) {
    return <Icon className={className} style={{ color }} size={size} />;
  }
  return <Icon className={cn(color, className)} size={size} />;
}
