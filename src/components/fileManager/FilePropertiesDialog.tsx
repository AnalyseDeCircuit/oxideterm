/**
 * FilePropertiesDialog Component
 * Native-style file properties dialog (Get Info / Properties)
 * Cross-platform: shows Unix permissions on macOS/Linux, read-only on Windows
 */

import React from 'react';
import { Folder, File, FileSymlink, Hash, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../ui/dialog';
import type { FileInfo, FileMetadata, ChecksumResult, DirStatsResult } from './types';
import {
  formatFileSize,
  formatOctalPermissions,
} from './utils';
import { platform } from '../../lib/platform';

export interface FilePropertiesDialogProps {
  open: boolean;
  onClose: () => void;
  file: FileInfo | null;
  metadata: FileMetadata | null;
  loading?: boolean;
  dirStats?: DirStatsResult | null;
  dirStatsLoading?: boolean;
  checksum?: ChecksumResult | null;
  checksumLoading?: boolean;
  onCalculateChecksum?: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

/** Format bytes with locale-aware thousand separators */
function formatExactBytes(bytes: number, t: FilePropertiesDialogProps['t'], locale: string): string {
  const formatted = bytes.toLocaleString(locale);
  return `${formatted} ${t('fileManager.propBytes')}`;
}

/** Full timestamp with weekday, following the app's i18n locale */
function formatFullTimestamp(timestamp: number | undefined, locale: string): string {
  if (!timestamp) return '-';
  return new Date(timestamp * 1000).toLocaleString(locale, {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/** Color-coded rwx permission display */
const permColor: Record<string, string> = {
  r: 'text-emerald-400',
  w: 'text-amber-400',
  x: 'text-sky-400',
  '-': 'text-zinc-600',
};

const ColoredPermissions: React.FC<{ mode: number }> = ({ mode }) => {
  const perms = mode & 0o777;
  const bits = [
    [0o400, 'r'], [0o200, 'w'], [0o100, 'x'],
    [0o040, 'r'], [0o020, 'w'], [0o010, 'x'],
    [0o004, 'r'], [0o002, 'w'], [0o001, 'x'],
  ] as const;

  return (
    <>
      {bits.map(([bit, ch], i) => {
        const active = (perms & bit) !== 0;
        const char = active ? ch : '-';
        return (
          <span key={i} className={permColor[char]}>
            {char}
          </span>
        );
      })}
    </>
  );
};

const PropertyRow: React.FC<{
  label: string;
  value: React.ReactNode;
  mono?: boolean;
}> = ({ label, value, mono }) => (
  <div className="flex items-start gap-3 py-1.5">
    <span className="text-zinc-500 text-xs shrink-0 min-w-[6.5rem] max-w-[8rem] text-right select-none break-keep">
      {label}
    </span>
    <span
      className={`text-zinc-200 text-xs break-all min-w-0 flex-1 select-text ${mono ? 'font-mono' : ''}`}
    >
      {value}
    </span>
  </div>
);

const Separator: React.FC = () => (
  <div className="border-t border-theme-border my-1.5" />
);

export const FilePropertiesDialog: React.FC<FilePropertiesDialogProps> = ({
  open,
  onClose,
  file,
  metadata,
  loading,
  dirStats,
  dirStatsLoading,
  checksum,
  checksumLoading,
  onCalculateChecksum,
  t,
}) => {
  if (!file) return null;

  // Use the app's i18n language for date/number formatting
  const { i18n } = useTranslation();
  const locale = i18n.language;

  const isDir = file.file_type === 'Directory';
  const isSymlink = file.file_type === 'Symlink';

  const FileIcon = isSymlink ? FileSymlink : isDir ? Folder : File;

  const dialogTitle = platform.isMac
    ? t('fileManager.propTitleGetInfo')
    : t('fileManager.properties');

  const fileType = isDir
    ? t('fileManager.propTypeFolder')
    : isSymlink
      ? t('fileManager.propTypeSymlink')
      : metadata?.mimeType || t('fileManager.propTypeFile');

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileIcon className="h-4 w-4 text-zinc-400 shrink-0" />
            <span className="truncate">{file.name}</span>
          </DialogTitle>
          <DialogDescription className="sr-only">
            {dialogTitle}
          </DialogDescription>
        </DialogHeader>

        <div className="px-4 py-3 space-y-0.5">
          {loading ? (
            <div className="flex items-center justify-center py-8 text-zinc-500 text-xs">
              {t('fileManager.loadingMore')}
            </div>
          ) : metadata ? (
            <>
              {/* General */}
              <PropertyRow
                label={t('fileManager.propKind')}
                value={fileType}
              />
              <PropertyRow
                label={t('fileManager.size')}
                value={
                  <span>
                    {formatFileSize(metadata.size)}
                    {metadata.size >= 1024 && (
                      <span className="text-zinc-600 ml-1">
                        ({formatExactBytes(metadata.size, t, locale)})
                      </span>
                    )}
                  </span>
                }
              />
              <PropertyRow
                label={t('fileManager.propLocation')}
                value={file.path}
              />

              <Separator />

              {/* Timestamps */}
              {metadata.created != null && (
                <PropertyRow
                  label={t('fileManager.created')}
                  value={formatFullTimestamp(metadata.created, locale)}
                />
              )}
              <PropertyRow
                label={t('fileManager.modified')}
                value={formatFullTimestamp(metadata.modified, locale)}
              />
              {metadata.accessed != null && (
                <PropertyRow
                  label={t('fileManager.propAccessed')}
                  value={formatFullTimestamp(metadata.accessed, locale)}
                />
              )}

              <Separator />

              {/* Permissions */}
              {metadata.mode !== undefined ? (
                /* Unix: show colored rwx + octal */
                <PropertyRow
                  label={t('fileManager.permissions')}
                  value={
                    <span>
                      <ColoredPermissions mode={metadata.mode} />
                      <span className="text-zinc-500 ml-1.5">
                        ({formatOctalPermissions(metadata.mode)})
                      </span>
                    </span>
                  }
                  mono
                />
              ) : (
                /* Windows: show read-only status */
                <PropertyRow
                  label={t('fileManager.propAccess')}
                  value={
                    metadata.readonly
                      ? t('fileManager.readonly')
                      : t('fileManager.readwrite')
                  }
                />
              )}

              {/* Symlink */}
              {metadata.isSymlink && (
                <PropertyRow
                  label={t('fileManager.symlink')}
                  value={t('fileManager.propYes')}
                />
              )}

              {/* MIME Type (files only) */}
              {!isDir && metadata.mimeType && (
                <PropertyRow
                  label={t('fileManager.mimeType')}
                  value={metadata.mimeType}
                  mono
                />
              )}

              {/* Directory Stats */}
              {isDir && (
                <>
                  <Separator />
                  {dirStatsLoading ? (
                    <PropertyRow
                      label={t('fileManager.propContents')}
                      value={
                        <span className="text-zinc-500 flex items-center gap-1.5">
                          <Loader2 className="h-3 w-3 animate-spin" />
                          {t('fileManager.propScanning')}
                        </span>
                      }
                    />
                  ) : dirStats ? (
                    <>
                      <PropertyRow
                        label={t('fileManager.propContents')}
                        value={t('fileManager.propDirSummary', {
                          files: dirStats.fileCount,
                          dirs: dirStats.dirCount,
                        })}
                      />
                      <PropertyRow
                        label={t('fileManager.propTotalSize')}
                        value={
                          <span>
                            {formatFileSize(dirStats.totalSize)}
                            {dirStats.totalSize >= 1024 && (
                              <span className="text-zinc-600 ml-1">
                                ({formatExactBytes(dirStats.totalSize, t, locale)})
                              </span>
                            )}
                          </span>
                        }
                      />
                    </>
                  ) : null}
                </>
              )}

              {/* Checksum (files only) */}
              {!isDir && (
                <>
                  <Separator />
                  {checksum ? (
                    <>
                      <PropertyRow label="MD5" value={checksum.md5} mono />
                      <PropertyRow label="SHA-256" value={checksum.sha256} mono />
                    </>
                  ) : (
                    <div className="flex items-start gap-3 py-1.5">
                      <span className="text-zinc-500 text-xs shrink-0 w-28 text-right select-none">
                        {t('fileManager.propChecksum')}
                      </span>
                      <button
                        className="text-xs text-sky-400 hover:text-sky-300 transition-colors flex items-center gap-1.5 disabled:opacity-50"
                        onClick={onCalculateChecksum}
                        disabled={checksumLoading}
                      >
                        {checksumLoading ? (
                          <>
                            <Loader2 className="h-3 w-3 animate-spin" />
                            {t('fileManager.propCalculating')}
                          </>
                        ) : (
                          <>
                            <Hash className="h-3 w-3" />
                            {t('fileManager.propCalcChecksum')}
                          </>
                        )}
                      </button>
                    </div>
                  )}
                </>
              )}
            </>
          ) : (
            <div className="flex items-center justify-center py-8 text-zinc-500 text-xs">
              {t('fileManager.error')}
            </div>
          )}
        </div>

        {/* Close button */}
        <div className="px-4 py-2.5 border-t border-theme-border bg-theme-bg-panel flex justify-end">
          <button
            className="px-3 py-1 text-xs rounded bg-zinc-700 hover:bg-zinc-600 text-zinc-200 transition-colors"
            onClick={onClose}
          >
            OK
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
};
