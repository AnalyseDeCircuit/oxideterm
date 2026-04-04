import { describe, expect, it } from 'vitest';
import {
  formatBytes,
  formatSpeed,
  calculateSpeed,
  type TransferItem,
} from '@/store/transferStore';

/** Helper to create a minimal TransferItem for calculateSpeed testing */
function makeTransfer(overrides: Partial<TransferItem> = {}): TransferItem {
  return {
    id: 'tx-1',
    nodeId: 'node-1',
    name: 'file.txt',
    localPath: '/tmp/file.txt',
    remotePath: '/home/user/file.txt',
    direction: 'upload',
    size: 1024,
    transferred: 0,
    state: 'active',
    startTime: Date.now(),
    ...overrides,
  } as TransferItem;
}

describe('formatBytes', () => {
  it('formats 0 bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
  });

  it('formats bytes', () => {
    expect(formatBytes(500)).toBe('500.0 B');
  });

  it('formats kilobytes', () => {
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1048576)).toBe('1.0 MB');
  });

  it('formats gigabytes', () => {
    expect(formatBytes(1073741824)).toBe('1.0 GB');
  });

  it('formats terabytes', () => {
    expect(formatBytes(1099511627776)).toBe('1.0 TB');
  });
});

describe('formatSpeed', () => {
  it('appends /s suffix', () => {
    expect(formatSpeed(1024)).toBe('1.0 KB/s');
  });

  it('handles zero', () => {
    expect(formatSpeed(0)).toBe('0 B/s');
  });
});

describe('calculateSpeed', () => {
  it('returns 0 for non-active transfers', () => {
    expect(calculateSpeed(makeTransfer({ state: 'completed' }))).toBe(0);
    expect(calculateSpeed(makeTransfer({ state: 'paused' }))).toBe(0);
    expect(calculateSpeed(makeTransfer({ state: 'pending' }))).toBe(0);
  });

  it('prefers backend-reported speed', () => {
    const transfer = makeTransfer({
      state: 'active',
      transferred: 5000,
      backendSpeed: 2048,
      startTime: Date.now() - 10000,
    });
    expect(calculateSpeed(transfer)).toBe(2048);
  });

  it('returns 0 when transferred is 0', () => {
    const transfer = makeTransfer({
      state: 'active',
      transferred: 0,
      startTime: Date.now() - 1000,
    });
    expect(calculateSpeed(transfer)).toBe(0);
  });

  it('calculates frontend speed from elapsed time', () => {
    const transfer = makeTransfer({
      state: 'active',
      transferred: 10000,
      startTime: Date.now() - 2000, // 2 seconds ago
    });
    const speed = calculateSpeed(transfer);
    // 10000 / 2 = 5000 (approximately, allowing for timing variance)
    expect(speed).toBeGreaterThan(4000);
    expect(speed).toBeLessThan(6000);
  });
});
