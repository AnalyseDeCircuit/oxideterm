import { describe, expect, it } from 'vitest';

import {
  createToolResultEnvelope,
  createToolTarget,
  detectTerminalPrompt,
  fromLegacyToolResult,
  hasTargetCapability,
  inferToolRisk,
  toLegacyToolResult,
} from '@/lib/ai/tools/protocol';
import type { AiToolResult } from '@/types';

describe('tool protocol v2 adapters', () => {
  it('maps an envelope to the legacy AiToolResult shape without dropping envelope data', () => {
    const envelope = createToolResultEnvelope({
      ok: true,
      toolName: 'terminal_exec',
      capability: 'command.run',
      targetId: 'node:demo',
      summary: 'Command completed',
      output: 'hello',
      data: { stdout: 'hello', exitCode: 0 },
      durationMs: 42,
    });

    const result = toLegacyToolResult(envelope, 'call-1');

    expect(result).toMatchObject({
      toolCallId: 'call-1',
      toolName: 'terminal_exec',
      success: true,
      output: 'hello',
      durationMs: 42,
      envelope,
    });
  });

  it('maps envelope errors to legacy error strings', () => {
    const envelope = createToolResultEnvelope({
      ok: false,
      toolName: 'write_file',
      summary: 'Hash mismatch',
      error: {
        code: 'expected_hash_mismatch',
        message: 'The file changed before writing.',
        recoverable: true,
      },
      durationMs: 5,
    });

    const result = toLegacyToolResult(envelope, 'call-error');

    expect(result.success).toBe(false);
    expect(result.output).toBe('Hash mismatch');
    expect(result.error).toBe('The file changed before writing.');
  });

  it('returns an existing envelope unchanged when converting from legacy result', () => {
    const envelope = createToolResultEnvelope({
      ok: true,
      toolName: 'read_file',
      summary: 'Read file',
      output: 'content',
    });
    const result: AiToolResult = {
      toolCallId: 'call-2',
      toolName: 'read_file',
      success: true,
      output: 'content',
      envelope,
    };

    expect(fromLegacyToolResult(result)).toBe(envelope);
  });

  it('creates a best-effort envelope from legacy success and failure results', () => {
    expect(fromLegacyToolResult({
      toolCallId: 'call-ok',
      toolName: 'get_terminal_buffer',
      success: true,
      output: 'first line\nsecond line',
      durationMs: 12,
    })).toMatchObject({
      ok: true,
      summary: 'first line',
      output: 'first line\nsecond line',
      meta: {
        toolName: 'get_terminal_buffer',
        durationMs: 12,
      },
    });

    expect(fromLegacyToolResult({
      toolCallId: 'call-fail',
      toolName: 'terminal_exec',
      success: false,
      output: '',
      error: 'Exit code: 1',
    })).toMatchObject({
      ok: false,
      summary: 'Exit code: 1',
      error: {
        code: 'legacy_tool_error',
        message: 'Exit code: 1',
        recoverable: true,
      },
    });
  });
});

describe('tool protocol risk and target helpers', () => {
  it('infers destructive command risk before generic terminal_exec risk', () => {
    expect(inferToolRisk('terminal_exec', { command: 'sudo reboot' })).toBe('destructive');
    expect(inferToolRisk('terminal_exec', { command: 'ls -la' })).toBe('execute-command');
    expect(inferToolRisk('terminal_exec', { session_id: 'session-1', command: 'vim file' })).toBe('interactive-input');
  });

  it('infers common capability and tool risks', () => {
    expect(inferToolRisk('read_file')).toBe('read');
    expect(inferToolRisk('write_file')).toBe('write-file');
    expect(inferToolRisk('create_port_forward')).toBe('network-expose');
    expect(inferToolRisk('update_setting')).toBe('settings-change');
    expect(inferToolRisk('custom_tool', {}, 'terminal.send')).toBe('interactive-input');
  });

  it('creates targets with optional fields only when provided', () => {
    const target = createToolTarget({
      id: 'session:abc',
      kind: 'terminal-session',
      label: 'work shell',
      active: true,
      sessionId: 'abc',
      capabilities: ['terminal.send', 'terminal.observe'],
    });

    expect(target).toEqual({
      id: 'session:abc',
      kind: 'terminal-session',
      label: 'work shell',
      active: true,
      sessionId: 'abc',
      capabilities: ['terminal.send', 'terminal.observe'],
    });
    expect(hasTargetCapability(target, 'terminal.observe')).toBe(true);
    expect(hasTargetCapability(target, 'filesystem.read')).toBe(false);
  });
});

describe('terminal protocol helpers', () => {
  it('detects shell and interactive input prompts from terminal text', () => {
    expect(detectTerminalPrompt('dominical@macbook %')).toEqual({
      kind: 'shell',
      text: 'dominical@macbook %',
    });
    expect(detectTerminalPrompt('sudo fastfetch\n[sudo] password for dominical:')).toEqual({
      kind: 'password',
      text: '[sudo] password for dominical:',
    });
    expect(detectTerminalPrompt('Enter passphrase for key /tmp/id_ed25519:')).toEqual({
      kind: 'passphrase',
      text: 'Enter passphrase for key /tmp/id_ed25519:',
    });
  });
});
