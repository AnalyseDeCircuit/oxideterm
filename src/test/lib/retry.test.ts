import { describe, expect, it, vi } from 'vitest';
import { retryWithExponentialBackoff } from '@/lib/retry';

describe('retryWithExponentialBackoff', () => {
  it('returns on first success', async () => {
    const fn = vi.fn().mockResolvedValue('ok');
    const result = await retryWithExponentialBackoff(fn);

    expect(result).toBe('ok');
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('retries on failure then succeeds', async () => {
    const fn = vi.fn()
      .mockRejectedValueOnce(new Error('fail'))
      .mockResolvedValueOnce('ok');

    const result = await retryWithExponentialBackoff(fn, {
      baseDelayMs: 1,
      maxDelayMs: 5,
    });

    expect(result).toBe('ok');
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('throws after exhausting retries', async () => {
    const fn = vi.fn().mockRejectedValue(new Error('always fail'));

    await expect(
      retryWithExponentialBackoff(fn, {
        maxRetries: 2,
        baseDelayMs: 1,
        maxDelayMs: 5,
      })
    ).rejects.toThrow('always fail');

    expect(fn).toHaveBeenCalledTimes(3); // initial + 2 retries
  });

  it('respects maxRetries=0 (no retry)', async () => {
    const fn = vi.fn().mockRejectedValue(new Error('fail'));

    await expect(
      retryWithExponentialBackoff(fn, { maxRetries: 0 })
    ).rejects.toThrow('fail');

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('respects shouldRetry filter', async () => {
    const fn = vi.fn().mockRejectedValue(new Error('non-retryable'));

    await expect(
      retryWithExponentialBackoff(fn, {
        maxRetries: 5,
        baseDelayMs: 1,
        shouldRetry: () => false,
      })
    ).rejects.toThrow('non-retryable');

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('passes error and attempt to shouldRetry', async () => {
    const shouldRetry = vi.fn().mockReturnValue(true);
    const error = new Error('test');
    const fn = vi.fn()
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce('ok');

    await retryWithExponentialBackoff(fn, {
      baseDelayMs: 1,
      shouldRetry,
    });

    expect(shouldRetry).toHaveBeenCalledWith(error, 0);
  });

  it('retries multiple times before success', async () => {
    const fn = vi.fn()
      .mockRejectedValueOnce(new Error('1'))
      .mockRejectedValueOnce(new Error('2'))
      .mockRejectedValueOnce(new Error('3'))
      .mockResolvedValueOnce('ok');

    const result = await retryWithExponentialBackoff(fn, {
      maxRetries: 5,
      baseDelayMs: 1,
      maxDelayMs: 5,
    });

    expect(result).toBe('ok');
    expect(fn).toHaveBeenCalledTimes(4);
  });
});
