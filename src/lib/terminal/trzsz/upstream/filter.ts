import { TextProgressBar } from '@/lib/terminal/trzsz/upstream/progress';
import { TrzszTransfer } from '@/lib/terminal/trzsz/upstream/transfer';
import type { TrzszOptions } from '@/lib/terminal/trzsz/upstream/options';
import {
  isTrzszCancelledError,
  type TrzszTransferDirection,
  type TrzszTransferEvent,
  type TrzszTransferPolicy,
} from '@/lib/terminal/trzsz/types';
import {
  checkDuplicateNames,
  formatSavedFiles,
  isArrayOfType,
  strToUint8,
  stripServerOutput,
  TrzszError,
  type TrzszFileReader,
  uint8ToStr,
} from '@/lib/terminal/trzsz/upstream/comm';

const TRZSZ_MAGIC_KEY_PREFIX = '::TRZSZ:TRANSFER:';
const TRZSZ_MAGIC_KEY_REGEXP = new RegExp(/::TRZSZ:TRANSFER:([SRD]):(\d+\.\d+\.\d+)(:\d+)?(?::\d+)?(?=[^0-9]|$)/);
const TRZSZ_MAGIC_ARRAY = new Float64Array(strToUint8(TRZSZ_MAGIC_KEY_PREFIX).buffer, 0, 2);
const MAGIC_DETECT_BUFFER_BYTES = 128;

function toDetectBytes(output: string | ArrayBuffer | Uint8Array | Blob): Uint8Array | null {
  if (typeof output === 'string') {
    return strToUint8(output);
  }

  if (output instanceof ArrayBuffer) {
    return new Uint8Array(output);
  }

  if (output instanceof Uint8Array) {
    return output;
  }

  return null;
}

export async function findTrzszMagicKey(
  output: string | ArrayBuffer | Uint8Array | Blob,
): Promise<string | null> {
  if (typeof output === 'string') {
    const index = output.lastIndexOf(TRZSZ_MAGIC_KEY_PREFIX);
    return index < 0 ? null : output.substring(index);
  }

  let bytes: Uint8Array;
  if (output instanceof ArrayBuffer) {
    bytes = new Uint8Array(output);
  } else if (output instanceof Uint8Array) {
    bytes = output;
  } else if (output instanceof Blob) {
    bytes = new Uint8Array(await output.arrayBuffer());
  } else {
    return null;
  }

  if (bytes.length < 26) {
    return null;
  }

  let index = -1;
  let found = -1;
  while (true) {
    index = bytes.indexOf(0x3a, index + 1);
    if (index < 0 || bytes.length - index < 26) {
      if (found >= 0) {
        return uint8ToStr(bytes.subarray(found));
      }
      return null;
    }

    const nextArray = new Float64Array(bytes.buffer.slice(bytes.byteOffset + index, bytes.byteOffset + index + 16));
    if (nextArray[0] === TRZSZ_MAGIC_ARRAY[0] && nextArray[1] === TRZSZ_MAGIC_ARRAY[1]) {
      found = index;
      index += 25;
    }
  }
}

export class TrzszFilter {
  private readonly writeToTerminal: (output: string | ArrayBuffer | Uint8Array | Blob) => void;
  private readonly sendToServer: (input: string | Uint8Array) => void;
  private readonly waitForTransferReady?: () => Promise<void>;
  private readonly chooseSendFiles: (directory?: boolean) => Promise<string[] | undefined>;
  private readonly buildFileReaders: (paths: string[], directory: boolean, policy: TrzszTransferPolicy) => Promise<TrzszFileReader[] | undefined>;
  private readonly chooseSaveDirectory: NonNullable<TrzszOptions['chooseSaveDirectory']>;
  private readonly createOpenSaveFile: NonNullable<TrzszOptions['createOpenSaveFile']>;
  private readonly getTransferPolicy: NonNullable<TrzszOptions['getTransferPolicy']>;
  private readonly onTransferEvent?: (event: TrzszTransferEvent) => void;
  private readonly terminalColumnsDefault: number;
  private readonly isWindowsShell: boolean;
  private readonly dragInitTimeout: number;
  private trzszTransfer: TrzszTransfer | null = null;
  private suppressCancelledEvent = false;
  private textProgressBar: TextProgressBar | null = null;
  private uniqueIdMaps = new Map<string, number>();
  private uploadFilesList: TrzszFileReader[] | null = null;
  private uploadFilesResolve: (() => void) | null = null;
  private uploadFilesReject: ((reason?: unknown) => void) | null = null;
  private uploadInterrupting = false;
  private uploadSkipTrzCommand = false;
  private disposed = false;
  private readonly pendingDetectTimers = new Set<ReturnType<typeof setTimeout>>();
  private readonly activeTasks = new Set<Promise<void>>();
  private uploadInitTimer: ReturnType<typeof setTimeout> | null = null;
  private detectTail = new Uint8Array(0);
  private handshakeLocked = false;
  private terminalColumns: number;

  constructor(options: TrzszOptions) {
    if (!options.writeToTerminal) {
      throw new TrzszError('TrzszOptions.writeToTerminal is required');
    }
    if (!options.sendToServer) {
      throw new TrzszError('TrzszOptions.sendToServer is required');
    }
    if (!options.chooseSendFiles) {
      throw new TrzszError('TrzszOptions.chooseSendFiles is required');
    }
    if (!options.buildFileReaders) {
      throw new TrzszError('TrzszOptions.buildFileReaders is required');
    }
    if (!options.chooseSaveDirectory) {
      throw new TrzszError('TrzszOptions.chooseSaveDirectory is required');
    }
    if (!options.createOpenSaveFile) {
      throw new TrzszError('TrzszOptions.createOpenSaveFile is required');
    }
    if (!options.getTransferPolicy) {
      throw new TrzszError('TrzszOptions.getTransferPolicy is required');
    }

    this.writeToTerminal = options.writeToTerminal;
    this.sendToServer = options.sendToServer;
  this.waitForTransferReady = options.waitForTransferReady;
    this.chooseSendFiles = options.chooseSendFiles;
    this.buildFileReaders = options.buildFileReaders;
    this.chooseSaveDirectory = options.chooseSaveDirectory;
    this.createOpenSaveFile = options.createOpenSaveFile;
    this.getTransferPolicy = options.getTransferPolicy;
    this.onTransferEvent = options.onTransferEvent;
    this.terminalColumnsDefault = options.terminalColumns || 80;
    this.terminalColumns = this.terminalColumnsDefault;
    this.isWindowsShell = options.isWindowsShell === true;
    this.dragInitTimeout = options.dragInitTimeout || 3000;
  }

  processServerOutput(output: string | ArrayBuffer | Uint8Array | Blob): void {
    if (this.disposed) {
      return;
    }

    if (this.isTransferringFiles()) {
      this.trzszTransfer?.addReceivedData(output);
      return;
    }

    if (this.uploadInterrupting) {
      return;
    }

    if (this.uploadSkipTrzCommand) {
      this.uploadSkipTrzCommand = false;
      const stripped = stripServerOutput(output);
      if (stripped === 'trz' || stripped === 'trz -d') {
        this.writeToTerminal('\r\n');
        return;
      }
    }

    const detectOutput = this.buildDetectOutput(output);
    const timerId = setTimeout(() => {
      this.pendingDetectTimers.delete(timerId);
      if (this.disposed) {
        return;
      }
      const task = this.detectAndHandleTrzsz(detectOutput);
      this.activeTasks.add(task);
      void task.finally(() => {
        this.activeTasks.delete(task);
      });
    }, 10);
    this.pendingDetectTimers.add(timerId);
    this.writeToTerminal(output);
  }

  processTerminalInput(input: string): void {
    if (this.disposed) {
      return;
    }

    if (this.isTransferringFiles()) {
      if (input === '\x03') {
        this.stopTransferringFiles();
      }
      return;
    }

    this.sendToServer(input);
  }

  processBinaryInput(input: string): void {
    if (this.disposed) {
      return;
    }

    if (this.isTransferringFiles()) {
      return;
    }

    this.sendToServer(strToUint8(input));
  }

  setTerminalColumns(columns: number): void {
    this.terminalColumns = columns;
    this.textProgressBar?.setTerminalColumns(columns);
  }

  isTransferringFiles(): boolean {
    return this.trzszTransfer !== null;
  }

  private emitTransferEvent(event: TrzszTransferEvent): void {
    this.onTransferEvent?.(event);
  }

  async dispose(): Promise<void> {
    this.disposed = true;
    this.clearPendingTimers();

    if (this.trzszTransfer) {
      this.suppressCancelledEvent = true;
      await this.trzszTransfer.stopTransferring();
    }

    if (this.activeTasks.size > 0) {
      await Promise.allSettled(Array.from(this.activeTasks));
    }
  }

  stopTransferringFiles(): void {
    if (!this.trzszTransfer) {
      return;
    }

    void this.trzszTransfer.stopTransferring();
  }

  async uploadFiles(items: string[] | DataTransferItemList): Promise<void> {
    if (this.uploadFilesList || this.isTransferringFiles()) {
      throw new Error('The previous upload has not been completed yet');
    }

    if (!isArrayOfType(items, 'string')) {
      throw new Error('The upload items type is not supported');
    }

    this.uploadFilesList = (await this.buildFileReaders(items as string[], true, this.getTransferPolicy())) ?? null;
    if (!this.uploadFilesList || this.uploadFilesList.length === 0) {
      this.uploadFilesList = null;
      throw new Error('No files to upload');
    }

    const hasDirectory = this.uploadFilesList.some((file) => file.isDir() || file.getRelPath().length > 1);
    this.uploadInterrupting = true;
    this.sendToServer('\x03');
    await new Promise((resolve) => setTimeout(resolve, 200));
    this.uploadInterrupting = false;

    this.uploadSkipTrzCommand = true;
    this.sendToServer(hasDirectory ? 'trz -d\r' : 'trz\r');

    this.uploadInitTimer = setTimeout(() => {
      this.uploadInitTimer = null;
      if (!this.uploadFilesList) {
        return;
      }

      this.uploadFilesList = null;
      this.uploadFilesResolve = null;
      this.uploadFilesReject?.('Upload does not start');
      this.uploadFilesReject = null;
    }, this.dragInitTimeout);

    return new Promise<void>((resolve, reject) => {
      this.uploadFilesResolve = resolve;
      this.uploadFilesReject = reject;
    });
  }

  private uniqueIdExists(uniqueId: string): boolean {
    if (uniqueId.length === 0) {
      return false;
    }
    if (!this.isWindowsShell && uniqueId.length === 14 && uniqueId.endsWith('00')) {
      return false;
    }
    if (this.uniqueIdMaps.has(uniqueId)) {
      return true;
    }
    if (this.uniqueIdMaps.size >= 100) {
      const nextMap = new Map<string, number>();
      for (const [key, value] of this.uniqueIdMaps.entries()) {
        if (value >= 50) {
          nextMap.set(key, value - 50);
        }
      }
      this.uniqueIdMaps = nextMap;
    }
    this.uniqueIdMaps.set(uniqueId, this.uniqueIdMaps.size);
    return false;
  }

  private async detectAndHandleTrzsz(output: string | ArrayBuffer | Uint8Array | Blob): Promise<void> {
    if (this.disposed || this.isTransferringFiles()) {
      return;
    }

    const buffer = await findTrzszMagicKey(output);
    if (this.disposed || !buffer) {
      return;
    }

    const found = buffer.match(TRZSZ_MAGIC_KEY_REGEXP);
    if (!found) {
      return;
    }

    const uniqueId = found.length > 3 ? (found[3] ?? '') : '';
    if (this.uniqueIdExists(uniqueId)) {
      return;
    }
    if (this.handshakeLocked) {
      return;
    }

    const mode = found[1];
    const version = found[2];
    const remoteIsWindows = uniqueId === ':1' || (uniqueId.length === 14 && uniqueId.endsWith('10'));
    const direction: TrzszTransferDirection = mode === 'S' ? 'download' : 'upload';
    let selection: 'file' | 'directory' = mode === 'D' ? 'directory' : 'file';
    const policy = this.getTransferPolicy();

    try {
      this.handshakeLocked = true;
      this.detectTail = new Uint8Array(0);
      this.trzszTransfer = new TrzszTransfer(this.sendToServer, this.isWindowsShell, policy.maxChunkBytes);
      this.trzszTransfer.setRemotePlatform(remoteIsWindows);
      await this.waitForTransferReady?.();
      if (this.disposed) {
        return;
      }
      if (mode === 'S') {
        selection = await this.handleTrzszDownloadFiles(version, remoteIsWindows, policy);
      } else if (mode === 'R') {
        await this.handleTrzszUploadFiles(version, false, remoteIsWindows, policy);
      } else if (mode === 'D') {
        await this.handleTrzszUploadFiles(version, true, remoteIsWindows, policy);
      }
      this.uploadFilesResolve?.();
      this.emitTransferEvent({ type: 'completed', direction, selection });
    } catch (error) {
      await this.trzszTransfer?.clientError(error instanceof Error ? error : new Error(String(error)));
      this.uploadFilesReject?.(error);
      if (isTrzszCancelledError(error)) {
        if (!this.suppressCancelledEvent) {
          this.emitTransferEvent({ type: 'cancelled', direction, selection });
        }
      } else {
        this.emitTransferEvent({ type: 'failed', direction, selection, error });
      }
    } finally {
      this.suppressCancelledEvent = false;
      this.handshakeLocked = false;
      this.uploadFilesResolve = null;
      this.uploadFilesReject = null;
      await this.trzszTransfer?.cleanup();
      this.textProgressBar?.showCursor();
      this.textProgressBar = null;
      this.trzszTransfer = null;
    }
  }

  private createProgressBar(quiet?: boolean, tmuxPaneColumns?: number): void {
    if (quiet === true) {
      this.textProgressBar = null;
      return;
    }

    this.textProgressBar = new TextProgressBar(this.writeToTerminalAsText, this.terminalColumns, tmuxPaneColumns);
    this.textProgressBar.hideCursor();
  }

  private async handleTrzszDownloadFiles(
    _version: string,
    remoteIsWindows: boolean,
    policy: TrzszTransferPolicy,
  ): Promise<'file' | 'directory'> {
    this.emitTransferEvent({ type: 'prompt', direction: 'download', selection: 'directory' });
    const saveRoot = await this.chooseSaveDirectory();
    if (this.disposed) {
      throw new TrzszError('Stopped');
    }
    if (!saveRoot) {
      await this.trzszTransfer?.sendAction(false, remoteIsWindows);
      throw new TrzszError('Stopped');
    }

    await this.trzszTransfer?.sendAction(true, remoteIsWindows);
    const config = await this.trzszTransfer?.recvConfig();
    const selection: 'file' | 'directory' = config?.directory === true ? 'directory' : 'file';
    if (selection === 'directory' && !policy.allowDirectory) {
      throw new TrzszError('Directory transfer is disabled', 'directory_not_allowed');
    }
    this.createProgressBar(config?.quiet === true, typeof config?.tmux_pane_width === 'number' ? config.tmux_pane_width : undefined);
    const localNames = await this.trzszTransfer?.recvFiles(
      saveRoot,
      this.createOpenSaveFile(policy),
      this.textProgressBar ?? null,
    );
    await this.trzszTransfer?.clientExit(formatSavedFiles(localNames ?? [], saveRoot.rootPath));
    return selection;
  }

  private async handleTrzszUploadFiles(
    _version: string,
    directory: boolean,
    remoteIsWindows: boolean,
    policy: TrzszTransferPolicy,
  ): Promise<void> {
    let sendFiles: TrzszFileReader[] | undefined;
    if (this.uploadFilesList) {
      sendFiles = this.uploadFilesList;
      this.uploadFilesList = null;
    } else {
      if (directory && !policy.allowDirectory) {
        throw new TrzszError('Directory transfer is disabled', 'directory_not_allowed');
      }

      this.emitTransferEvent({
        type: 'prompt',
        direction: 'upload',
        selection: directory ? 'directory' : 'file',
      });
      const filePaths = await this.chooseSendFiles(directory);
      if (this.disposed) {
        throw new TrzszError('Stopped');
      }
      if (filePaths) {
        sendFiles = await this.buildFileReaders(filePaths, directory, policy);
        if (this.disposed) {
          throw new TrzszError('Stopped');
        }
      }
    }

    if (!sendFiles || sendFiles.length === 0) {
      await this.trzszTransfer?.sendAction(false, remoteIsWindows);
      throw new TrzszError('Stopped');
    }

    await this.trzszTransfer?.sendAction(true, remoteIsWindows);
    const config = await this.trzszTransfer?.recvConfig();
    if (config?.overwrite === true) {
      checkDuplicateNames(sendFiles);
    }

    this.createProgressBar(config?.quiet === true, typeof config?.tmux_pane_width === 'number' ? config.tmux_pane_width : undefined);
    const remoteNames = await this.trzszTransfer?.sendFiles(sendFiles, this.textProgressBar ?? null);
    await this.trzszTransfer?.clientExit(formatSavedFiles(remoteNames ?? [], ''));
  }

  private readonly writeToTerminalAsText = (output: string): void => {
    this.writeToTerminal(output);
  };

  private clearPendingTimers(): void {
    for (const timerId of this.pendingDetectTimers) {
      clearTimeout(timerId);
    }
    this.pendingDetectTimers.clear();
    if (this.uploadInitTimer) {
      clearTimeout(this.uploadInitTimer);
      this.uploadInitTimer = null;
    }
    this.detectTail = new Uint8Array(0);
  }

  private buildDetectOutput(output: string | ArrayBuffer | Uint8Array | Blob): string | ArrayBuffer | Uint8Array | Blob {
    const bytes = toDetectBytes(output);
    if (!bytes) {
      return output;
    }

    const combined = new Uint8Array(this.detectTail.length + bytes.length);
    combined.set(this.detectTail, 0);
    combined.set(bytes, this.detectTail.length);
    const start = Math.max(0, combined.length - MAGIC_DETECT_BUFFER_BYTES);
    this.detectTail = combined.slice(start);
    return combined;
  }
}
