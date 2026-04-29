import { describe, expect, it } from 'vitest';

import { actionResultToToolResult } from '@/lib/ai/orchestrator/result';

describe('orchestrator result output handling', () => {
  it('keeps small output as the model output without rawOutput duplication', () => {
    const result = actionResultToToolResult('call-small', 'run_command', {
      ok: true,
      summary: 'Command completed.',
      output: 'x'.repeat(10 * 1024),
      risk: 'execute',
    }, 1);

    expect(result.truncated).toBe(false);
    expect(result.envelope?.rawOutput).toBeUndefined();
    expect(result.envelope?.outputPreview).toMatchObject({
      strategy: 'full',
      charCount: 10 * 1024,
      rawOutputStored: false,
    });
  });

  it('stores full medium output for UI while sending a head/tail preview', () => {
    const output = [
      'HEAD',
      'x'.repeat(100 * 1024),
      'TAIL',
    ].join('\n');
    const result = actionResultToToolResult('call-medium', 'run_command', {
      ok: true,
      summary: 'Command completed.',
      output,
      risk: 'execute',
    }, 1);

    expect(result.truncated).toBe(true);
    expect(result.output.length).toBeLessThan(13_000);
    expect(result.output).toContain('HEAD');
    expect(result.output).toContain('TAIL');
    expect(result.envelope?.rawOutput).toBe(output);
    expect(result.envelope?.outputPreview).toMatchObject({
      strategy: 'head_tail',
      rawOutputStored: true,
    });
  });

  it('does not persist very large raw output', () => {
    const output = [
      'HEAD',
      'x'.repeat(500 * 1024),
      'TAIL',
    ].join('\n');
    const result = actionResultToToolResult('call-large', 'run_command', {
      ok: true,
      summary: 'Command completed.',
      output,
      risk: 'execute',
    }, 1);

    expect(result.truncated).toBe(true);
    expect(result.output).toContain('HEAD');
    expect(result.output).toContain('TAIL');
    expect(result.envelope?.rawOutput).toBeUndefined();
    expect(result.envelope?.outputPreview).toMatchObject({
      strategy: 'head_tail',
      rawOutputStored: false,
    });
    expect(result.envelope?.warnings?.[0]).toContain('Full output exceeded');
  });
});
