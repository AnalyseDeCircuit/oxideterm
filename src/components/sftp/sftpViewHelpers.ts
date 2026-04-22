import type { TransferDirection, TransferItem } from '@/store/transferStore';
import type { FileInfo } from '@/types';

type ProgressMatchCandidate = Pick<TransferItem, 'id' | 'localPath' | 'remotePath'>;

export type ProgressEventMatchPayload = {
  id: string;
  local_path: string;
  remote_path: string;
};

export type PreviewResource = {
  tempPath?: string;
} | null;

export type SftpPaneTarget = 'local' | 'remote';

export type SftpInternalDragDropData = {
  files: string[];
  source: SftpPaneTarget;
  basePath: string;
};

export type RectLike = {
  left: number;
  right: number;
  top: number;
  bottom: number;
};

export type ExternalPathStat = {
  isDirectory: boolean;
  isSymlink: boolean;
  size: number;
  mtime: Date | null;
};

export type ExternalUploadCandidate = {
  file: string;
  sourcePath: string;
  fileInfo: FileInfo;
};

function isDefined<T>(value: T | null): value is T {
  return value !== null;
}

export function normalizeSftpTransferPath(path: string): string {
  if (!path) {
    return '';
  }

  const normalized = path.replace(/\/+/g, '/').replace(/\/$/, '');
  return normalized || '/';
}

export function parseSftpInternalDragDropData(raw: string): SftpInternalDragDropData | null {
  if (!raw) {
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<SftpInternalDragDropData>;
    if (
      !Array.isArray(parsed.files) ||
      (parsed.source !== 'local' && parsed.source !== 'remote') ||
      typeof parsed.basePath !== 'string'
    ) {
      return null;
    }

    const files = parsed.files.filter((file): file is string => typeof file === 'string' && file.length > 0);
    if (files.length === 0) {
      return null;
    }

    return {
      files,
      source: parsed.source,
      basePath: parsed.basePath,
    };
  } catch {
    return null;
  }
}

export function getSftpPaneTargetFromPhysicalPosition(
  position: { x: number; y: number },
  panes: Record<SftpPaneTarget, RectLike | null>,
  scaleFactor = 1,
): SftpPaneTarget | null {
  const cssX = position.x / scaleFactor;
  const cssY = position.y / scaleFactor;

  for (const pane of ['remote', 'local'] satisfies SftpPaneTarget[]) {
    const rect = panes[pane];
    if (!rect) {
      continue;
    }
    if (cssX >= rect.left && cssX <= rect.right && cssY >= rect.top && cssY <= rect.bottom) {
      return pane;
    }
  }

  return null;
}

function normalizeExternalDroppedPath(path: string): string {
  if (!path) {
    return '';
  }

  if (/^[A-Za-z]:[\\/]*$/.test(path)) {
    return `${path[0]}:\\`;
  }

  if (/^[\\/]+$/.test(path)) {
    return path[0];
  }

  return path.replace(/[\\/]+$/, '');
}

function getPathBaseName(path: string): string {
  const normalized = normalizeExternalDroppedPath(path);
  const segments = normalized.split(/[\\/]+/).filter(Boolean);
  return segments.at(-1) ?? '';
}

export async function buildExternalUploadCandidates(
  paths: string[],
  statPath: (path: string) => Promise<ExternalPathStat>,
): Promise<ExternalUploadCandidate[]> {
  const uniquePaths = Array.from(
    new Set(paths.map(normalizeExternalDroppedPath).filter((path) => path.length > 0)),
  );

  const candidates = await Promise.all(uniquePaths.map(async (path) => {
    const name = getPathBaseName(path);
    if (!name) {
      return null;
    }

    const info = await statPath(path);
    return {
      file: name,
      sourcePath: path,
      fileInfo: {
        name,
        path,
        file_type: info.isDirectory ? 'Directory' : info.isSymlink ? 'Symlink' : 'File',
        size: info.size || 0,
        modified: info.mtime ? Math.floor(info.mtime.getTime() / 1000) : null,
        permissions: null,
      } satisfies FileInfo,
    } satisfies ExternalUploadCandidate;
  }));

  return candidates.filter(isDefined);
}

function findUniquePathCandidate<T extends ProgressMatchCandidate>(
  transfers: T[],
  key: 'localPath' | 'remotePath',
  targetPath: string,
): T | undefined {
  const candidates = transfers.filter(
    (transfer) => normalizeSftpTransferPath(transfer[key]) === targetPath,
  );

  return candidates.length === 1 ? candidates[0] : undefined;
}

export function findTransferForProgressEvent<T extends ProgressMatchCandidate>(
  transfers: T[],
  event: ProgressEventMatchPayload,
): T | undefined {
  const byId = transfers.find((transfer) => transfer.id === event.id);
  if (byId) {
    return byId;
  }

  const normalizedRemote = normalizeSftpTransferPath(event.remote_path);
  const normalizedLocal = normalizeSftpTransferPath(event.local_path);

  const exactPathMatch = transfers.find((transfer) => {
    const transferRemote = normalizeSftpTransferPath(transfer.remotePath);
    const transferLocal = normalizeSftpTransferPath(transfer.localPath);
    return transferRemote === normalizedRemote && transferLocal === normalizedLocal;
  });

  if (exactPathMatch) {
    return exactPathMatch;
  }

  const byRemote = findUniquePathCandidate(transfers, 'remotePath', normalizedRemote);
  if (byRemote) {
    return byRemote;
  }

  return findUniquePathCandidate(transfers, 'localPath', normalizedLocal);
}

export function getTransferCompletionRefreshPlan(direction?: TransferDirection | null): {
  refreshLocal: boolean;
  refreshRemote: boolean;
} {
  if (direction === 'upload') {
    return { refreshLocal: false, refreshRemote: true };
  }

  if (direction === 'download') {
    return { refreshLocal: true, refreshRemote: false };
  }

  return { refreshLocal: true, refreshRemote: true };
}

export async function cleanupPreviewResource(
  preview: PreviewResource,
  cleanup: (path: string) => Promise<unknown>,
): Promise<void> {
  if (!preview?.tempPath) {
    return;
  }

  try {
    await cleanup(preview.tempPath);
  } catch {
    // Best-effort temp cleanup; preview closing should not fail the UI flow.
  }
}
