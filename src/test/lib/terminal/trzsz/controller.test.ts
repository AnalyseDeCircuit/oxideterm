import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createUnavailableTrzszCapabilities,
  type TrzszCapabilitiesProbeResult,
} from '@/lib/terminal/trzsz/capabilities';
import { chooseSaveRoot } from '@/lib/terminal/trzsz/dialogs';
import { notifyTrzszTransferEvent } from '@/lib/terminal/trzsz/notifications';
import { TrzszController } from '@/lib/terminal/trzsz/controller';
import type { RemoteTerminalTransport } from '@/lib/terminal/trzsz/transport';
import type { TrzszSaveRoot } from '@/lib/terminal/trzsz/types';
import type { InBandTransferSettings } from '@/store/settingsStore';

vi.mock('@/lib/terminal/trzsz/dialogs', () => ({
  chooseSendEntries: vi.fn(),
  chooseSaveRoot: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: {
    trzszPrepareDownloadRoot: vi.fn(async (_ownerId: string, rootPath: string) => ({ rootPath })),
  },
}));

vi.mock('@/lib/terminal/trzsz/notifications', () => ({
  notifyTrzszTransferEvent: vi.fn(),
}));

const transferSettings: InBandTransferSettings = {
  enabled: true,
  provider: 'trzsz',
  allowDirectory: true,
  maxChunkBytes: 1024 * 1024,
  maxFileCount: 1024,
  maxTotalBytes: 10 * 1024 * 1024 * 1024,
};

const availableCapabilities: TrzszCapabilitiesProbeResult = {
  status: 'available',
  capabilities: {
    apiVersion: 1,
    provider: 'trzsz',
    features: {
      directory: true,
      atomicDirectoryStage: true,
    },
  },
};

function createTransportMock(): RemoteTerminalTransport {
  return {
    canSendInput: vi.fn(() => true),
    sendEncodedPayload: vi.fn(() => true),
    sendRawInput: vi.fn(() => true),
    sendTextInput: vi.fn(() => true),
    sendExecuteInput: vi.fn(() => true),
    sendBinaryInput: vi.fn(() => true),
    sendResize: vi.fn(() => true),
  };
}

async function flushMicrotasks() {
  await Promise.resolve();
}

describe('TrzszController', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.mocked(chooseSaveRoot).mockReset();
    vi.mocked(chooseSaveRoot).mockResolvedValue(undefined);
    vi.mocked(notifyTrzszTransferEvent).mockReset();
  });

  it('passes server output to the writer and forwards input while active', () => {
    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-1',
      connectionId: 'conn-1',
      wsUrl: 'ws://localhost:1234',
      ownerId: 'trzsz:session-1:conn-1:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new Uint8Array([0x61, 0x62]));
    controller.processTerminalInput('ls');
    controller.processBinaryInput('\u001bOA');
    controller.sendTextInput('echo hello');
    controller.sendExecuteInput('pwd');
    controller.setTerminalColumns(132);

    expect(writeServerOutput).toHaveBeenCalledWith(new Uint8Array([0x61, 0x62]));
    expect(transport.sendRawInput).toHaveBeenCalledWith('ls');
    expect(transport.sendTextInput).toHaveBeenCalledWith('echo hello');
    expect(transport.sendExecuteInput).toHaveBeenCalledWith('pwd');
    expect(transport.sendEncodedPayload).toHaveBeenCalledWith(new Uint8Array([0x1b, 0x4f, 0x41]));
    expect(controller.getTerminalColumns()).toBe(132);
    expect(controller.matchesRuntime('conn-1', 'ws://localhost:1234')).toBe(true);
    expect(cleanupOwner).not.toHaveBeenCalled();
  });

  it('stops processing IO once draining or disposed', async () => {
    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-1',
      connectionId: 'conn-1',
      wsUrl: 'ws://localhost:1234',
      ownerId: 'trzsz:session-1:conn-1:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.stop();
    expect(controller.isDraining()).toBe(true);
    expect(controller.processTerminalInput('pwd')).toBe(false);
    expect(controller.sendTextInput('echo blocked')).toBe(false);
    expect(controller.sendExecuteInput('blocked')).toBe(false);
    controller.processServerOutput(new Uint8Array([0x63]));

    controller.dispose();
    await flushMicrotasks();

    expect(controller.isDisposed()).toBe(true);
    expect(controller.processBinaryInput('\u001bOB')).toBe(false);
    expect(writeServerOutput).not.toHaveBeenCalled();
    expect(transport.sendRawInput).not.toHaveBeenCalled();
    expect(transport.sendEncodedPayload).not.toHaveBeenCalled();
    expect(cleanupOwner).toHaveBeenCalledTimes(1);
  });

  it('still allows cleanup protocol frames while draining', () => {
    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-cleanup',
      connectionId: 'conn-cleanup',
      wsUrl: 'ws://localhost:1357',
      ownerId: 'trzsz:session-cleanup:conn-cleanup:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.stop();
    (controller as unknown as { filter: { sendToServer: (input: string) => void } }).filter.sendToServer('#FAIL:cleanup\n');

    expect(transport.sendRawInput).toHaveBeenCalledWith('#FAIL:cleanup\n');
  });

  it('stores capability probe results without breaking passthrough mode', async () => {
    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const available: TrzszCapabilitiesProbeResult = {
      status: 'available',
      capabilities: {
        ...availableCapabilities.capabilities,
        features: {
          directory: false,
          atomicDirectoryStage: false,
        },
      },
    };
    const controller = new TrzszController({
      sessionId: 'session-2',
      connectionId: 'conn-2',
      wsUrl: 'ws://localhost:4321',
      ownerId: 'trzsz:session-2:conn-2:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => available,
      cleanupOwner,
      transferSettings,
    });

    await flushMicrotasks();

    expect(controller.getCapabilities()).toEqual(available);
    controller.processTerminalInput('echo ready');
    expect(transport.sendRawInput).toHaveBeenCalledWith('echo ready');
  });

  it('fails transfer handshakes when the backend capability version mismatches', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const mismatch: TrzszCapabilitiesProbeResult = {
      status: 'available',
      capabilities: {
        ...availableCapabilities.capabilities,
        apiVersion: 2,
      },
    };
    const controller = new TrzszController({
      sessionId: 'session-mismatch',
      connectionId: 'conn-mismatch',
      wsUrl: 'ws://localhost:7001',
      ownerId: 'trzsz:session-mismatch:conn-mismatch:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => mismatch,
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new TextEncoder().encode('::TRZSZ:TRANSFER:S:1.1.6:12345678\r\n'));

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    expect(controller.getCapabilities()).toEqual(mismatch);
    expect(vi.mocked(chooseSaveRoot)).not.toHaveBeenCalled();
    expect(transport.sendRawInput).toHaveBeenCalledWith(expect.stringContaining('#fail:'));
    expect(transport.sendEncodedPayload).not.toHaveBeenCalled();
    expect(vi.mocked(notifyTrzszTransferEvent)).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'failed',
        error: expect.objectContaining({
          message: expect.stringContaining('invalid_api_version'),
        }),
      }),
    );
  });

  it('keeps cleanup protocol pinned to the original transport after the controller runtime is invalidated', async () => {
    vi.useFakeTimers();

	    const writeServerOutput = vi.fn();
	    const transport = createTransportMock();
	    const cleanupOwner = vi.fn(async () => undefined);
	    let runtimeCurrent = true;
	    let finishSaveRootSelection!: (value: TrzszSaveRoot | undefined) => void;
	    const saveRootSelection = new Promise<TrzszSaveRoot | undefined>((resolve) => {
	      finishSaveRootSelection = resolve;
	    });
	    vi.mocked(chooseSaveRoot).mockImplementation(() => saveRootSelection);

    const controller = new TrzszController({
      sessionId: 'session-stale',
      connectionId: 'conn-stale',
      wsUrl: 'ws://localhost:7002',
      ownerId: 'trzsz:session-stale:conn-stale:owner',
      isRuntimeCurrent: () => runtimeCurrent,
      transport,
      writeServerOutput,
      loadCapabilities: async () => availableCapabilities,
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new TextEncoder().encode('::TRZSZ:TRANSFER:S:1.1.6:12345678\r\n'));

    await vi.runAllTimersAsync();
	    await flushMicrotasks();
		    vi.mocked(notifyTrzszTransferEvent).mockClear();
		
		    runtimeCurrent = false;
		    controller.dispose();
		    finishSaveRootSelection({
		      rootPath: '/tmp/trzsz-downloads',
		      displayName: 'downloads',
		      maps: new Map(),
		    });

    await vi.runAllTimersAsync();
    await flushMicrotasks();
    await flushMicrotasks();

    expect(transport.sendRawInput).toHaveBeenCalledWith(expect.stringContaining('#fail:'));
    expect(transport.sendEncodedPayload).not.toHaveBeenCalled();
    expect(vi.mocked(notifyTrzszTransferEvent)).not.toHaveBeenCalled();
    expect(cleanupOwner).toHaveBeenCalledTimes(1);
  });

  it('suppresses partial cleanup toasts after the runtime is invalidated', async () => {
    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    let runtimeCurrent = true;
    const cleanupOwner = vi.fn(async () => {
      throw new Error('cleanup failed');
    });
    const controller = new TrzszController({
      sessionId: 'session-cleanup-toast',
      connectionId: 'conn-cleanup-toast',
      wsUrl: 'ws://localhost:7003',
      ownerId: 'trzsz:session-cleanup-toast:conn-cleanup-toast:owner',
      isRuntimeCurrent: () => runtimeCurrent,
      transport,
      writeServerOutput,
      loadCapabilities: async () => availableCapabilities,
      cleanupOwner,
      transferSettings,
    });

    runtimeCurrent = false;
    controller.dispose();
    await flushMicrotasks();
    await flushMicrotasks();

    expect(cleanupOwner).toHaveBeenCalledTimes(1);
    expect(vi.mocked(notifyTrzszTransferEvent)).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'partial_cleanup' }),
    );
  });

  it('does not start delayed trzsz detection after disposal', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-3',
      connectionId: 'conn-3',
      wsUrl: 'ws://localhost:2468',
      ownerId: 'trzsz:session-3:conn-3:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(
      new TextEncoder().encode('::TRZSZ:TRANSFER:S:1.1.6:12345678'),
    );
    controller.dispose();

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    expect(transport.sendRawInput).not.toHaveBeenCalled();
    expect(transport.sendEncodedPayload).not.toHaveBeenCalled();
  });

  it('detects a trzsz handshake split across multiple server chunks', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-4',
      connectionId: 'conn-4',
      wsUrl: 'ws://localhost:9999',
      ownerId: 'trzsz:session-4:conn-4:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new TextEncoder().encode('::TRZSZ:TRANS'));
    controller.processServerOutput(new TextEncoder().encode('FER:S:1.1.6:12345678'));
    controller.processServerOutput(new TextEncoder().encode('\r\n'));

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    expect(transport.sendRawInput).toHaveBeenCalled();
  });

  it('detects Go trzsz 1.2.0 handshakes with tunnel port metadata', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-go-1',
      connectionId: 'conn-go-1',
      wsUrl: 'ws://localhost:7777',
      ownerId: 'trzsz:session-go-1:conn-go-1:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(
      new TextEncoder().encode('\x1b[s::TRZSZ:TRANSFER:R:1.2.0:1234567890000:0\r\n'),
    );

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    const actionFrames = vi.mocked(transport.sendRawInput).mock.calls.filter(
      ([payload]) => typeof payload === 'string' && payload.startsWith('#ACT:'),
    );
    expect(actionFrames).toHaveLength(1);
  });

  it('detects a trzsz 1.2.0 handshake that ends with a dangling colon', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-go-2',
      connectionId: 'conn-go-2',
      wsUrl: 'ws://localhost:6666',
      ownerId: 'trzsz:session-go-2:conn-go-2:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new TextEncoder().encode('::TRZSZ:TRANSFER:R:1.2.0:\r\n'));

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    const actionFrames = vi.mocked(transport.sendRawInput).mock.calls.filter(
      ([payload]) => typeof payload === 'string' && payload.startsWith('#ACT:'),
    );
    expect(actionFrames).toHaveLength(1);
  });

  it('deduplicates Windows-style short unique ids across follow-up chunks', async () => {
    vi.useFakeTimers();

    const writeServerOutput = vi.fn();
    const transport = createTransportMock();
    const cleanupOwner = vi.fn(async () => undefined);
    const controller = new TrzszController({
      sessionId: 'session-5',
      connectionId: 'conn-5',
      wsUrl: 'ws://localhost:8888',
      ownerId: 'trzsz:session-5:conn-5:owner',
      isRuntimeCurrent: () => true,
      transport,
      writeServerOutput,
      loadCapabilities: async () => createUnavailableTrzszCapabilities('command-missing'),
      cleanupOwner,
      transferSettings,
    });

    controller.processServerOutput(new TextEncoder().encode('::TRZSZ:TRANS'));
    controller.processServerOutput(new TextEncoder().encode('FER:S:1.1.6:1'));
    controller.processServerOutput(new TextEncoder().encode('\r\n'));

    await vi.runAllTimersAsync();
    await flushMicrotasks();

    expect(transport.sendRawInput).toHaveBeenCalledTimes(2);
    const actionFrames = vi.mocked(transport.sendRawInput).mock.calls.filter(
      ([payload]) => typeof payload === 'string' && payload.startsWith('#ACT:'),
    );
    expect(actionFrames).toHaveLength(1);
  });
});
