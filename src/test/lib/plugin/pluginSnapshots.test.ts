import { describe, expect, it } from 'vitest';
import { freezeSnapshot, sanitizeForPlugin } from '@/lib/plugin/pluginSnapshots';

describe('freezeSnapshot', () => {
  it('deep freezes a nested object', () => {
    const obj = { a: 1, nested: { b: 2, deep: { c: 3 } } };
    const frozen = freezeSnapshot(obj);

    expect(Object.isFrozen(frozen)).toBe(true);
    expect(Object.isFrozen(frozen.nested)).toBe(true);
    expect(Object.isFrozen(frozen.nested.deep)).toBe(true);
  });

  it('returns same reference', () => {
    const obj = { x: 1 };
    expect(freezeSnapshot(obj)).toBe(obj);
  });

  it('handles null', () => {
    expect(freezeSnapshot(null)).toBeNull();
  });

  it('handles undefined', () => {
    expect(freezeSnapshot(undefined)).toBeUndefined();
  });

  it('handles primitive values', () => {
    expect(freezeSnapshot(42)).toBe(42);
    expect(freezeSnapshot('hello')).toBe('hello');
    expect(freezeSnapshot(true)).toBe(true);
  });

  it('freezes arrays and their object elements', () => {
    const arr = [{ id: 1 }, { id: 2 }];
    freezeSnapshot(arr);

    expect(Object.isFrozen(arr)).toBe(true);
    expect(Object.isFrozen(arr[0])).toBe(true);
    expect(Object.isFrozen(arr[1])).toBe(true);
  });

  it('skips already frozen nested objects', () => {
    const inner = Object.freeze({ value: 'frozen' });
    const obj = { inner };
    freezeSnapshot(obj);

    expect(Object.isFrozen(obj)).toBe(true);
    expect(Object.isFrozen(obj.inner)).toBe(true);
  });

  it('handles empty object', () => {
    const obj = {};
    expect(Object.isFrozen(freezeSnapshot(obj))).toBe(true);
  });
});

describe('sanitizeForPlugin', () => {
  it('redacts specified string keys', () => {
    const obj = { password: 'secret', name: 'test' };
    const result = sanitizeForPlugin(obj, ['password']);

    expect(result.password).toBe('[redacted]');
    expect(result.name).toBe('test');
  });

  it('does not redact non-string values', () => {
    const obj = { count: 42, token: 'abc' };
    const result = sanitizeForPlugin(obj, ['count', 'token']);

    expect(result.count).toBe(42); // number, not redacted
    expect(result.token).toBe('[redacted]'); // string, redacted
  });

  it('returns a shallow copy', () => {
    const obj = { a: 1, b: 2 };
    const result = sanitizeForPlugin(obj, []);

    expect(result).toEqual(obj);
    expect(result).not.toBe(obj);
  });

  it('handles empty redactKeys', () => {
    const obj = { key: 'value' };
    const result = sanitizeForPlugin(obj, []);

    expect(result.key).toBe('value');
  });

  it('handles multiple redact keys', () => {
    const obj = { password: 'p', token: 't', apiKey: 'k', name: 'ok' };
    const result = sanitizeForPlugin(obj, ['password', 'token', 'apiKey']);

    expect(result.password).toBe('[redacted]');
    expect(result.token).toBe('[redacted]');
    expect(result.apiKey).toBe('[redacted]');
    expect(result.name).toBe('ok');
  });

  it('ignores redactKeys not present in object', () => {
    const obj = { name: 'ok' };
    const result = sanitizeForPlugin(obj, ['nonexistent']);

    expect(result.name).toBe('ok');
  });
});
