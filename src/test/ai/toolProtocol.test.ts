import { describe, expect, it } from 'vitest';

import {
  buildCapabilityStatuses,
  buildFileDiffSummary,
  buildToolTargets,
  byteLengthOfText,
  createToolResultEnvelope,
  createToolTarget,
  decideToolApproval,
  detectTerminalPrompt,
  fromLegacyToolResult,
  hasTargetCapability,
  inferToolRisk,
  sanitizeToolArguments,
  parseFileWriteRequest,
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

  it('treats credential-like arguments as high-risk and redacts them for display', () => {
    expect(inferToolRisk('send_keys', { keys: ['password'] })).toBe('credential-sensitive');
    expect(inferToolRisk('local_exec', { command: 'echo sk-secret-token' })).toBe('credential-sensitive');
    expect(sanitizeToolArguments({
      password: 'hunter2',
      keys: ['password', 'Enter'],
      nested: { apiKey: 'sk-test12345678' },
    })).toEqual({
      password: '[redacted]',
      keys: ['[redacted]', 'Enter'],
      nested: { apiKey: '[redacted]' },
    });
  });

  it('blocks high-risk calls from ordinary auto-approval', () => {
    expect(decideToolApproval({
      toolName: 'terminal_exec',
      args: { command: 'ls -la' },
      autoApproveTools: { terminal_exec: true },
    })).toMatchObject({
      risk: 'execute-command',
      autoApprove: true,
    });

    expect(decideToolApproval({
      toolName: 'terminal_exec',
      args: { command: 'sudo reboot' },
      autoApproveTools: { terminal_exec: true },
    })).toMatchObject({
      risk: 'destructive',
      autoApprove: false,
      requiresApproval: true,
      reason: 'high-risk',
    });

    expect(decideToolApproval({
      toolName: 'create_port_forward',
      args: {},
      autoApproveTools: { create_port_forward: true },
      autonomyLevel: 'autonomous',
    })).toMatchObject({
      risk: 'network-expose',
      autoApprove: false,
      requiresApproval: true,
    });

    expect(decideToolApproval({
      toolName: 'read_file',
      args: { path: '/tmp/a.txt' },
      autoApproveTools: { read_file: false },
      readOnlyTools: new Set(['read_file']),
    })).toMatchObject({
      risk: 'read',
      autoApprove: false,
      requiresApproval: true,
      reason: 'manual',
    });
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

describe('tool target discovery helpers', () => {
  it('builds unified targets for local shell, SSH nodes, terminal sessions, and tabs', () => {
    const targets = buildToolTargets({
      activeTabId: 'tab-ssh',
      localTerminals: new Map([
        ['local-1', { running: true, shell: { label: 'Zsh' } }],
      ]),
      sshNodes: [
        {
          id: 'node-1',
          host: 'example.com',
          username: 'root',
          port: 22,
          runtime: {
            status: 'connected',
            connectionId: 'conn-1',
            terminalIds: ['term-1'],
            sftpSessionId: 'sftp-1',
          },
        },
      ],
      tabs: [
        {
          id: 'tab-ssh',
          type: 'terminal',
          title: 'SSH',
          nodeId: 'node-1',
          sessionId: 'term-1',
        } as never,
        {
          id: 'tab-sftp',
          type: 'sftp',
          title: 'SFTP',
          nodeId: 'node-1',
        } as never,
      ],
    });

    expect(targets).toEqual(expect.arrayContaining([
      expect.objectContaining({
        id: 'local-shell:default',
        kind: 'local-shell',
        capabilities: expect.arrayContaining(['command.run']),
      }),
      expect.objectContaining({
        id: 'ssh-node:node-1',
        kind: 'ssh-node',
        nodeId: 'node-1',
        active: true,
        capabilities: expect.arrayContaining(['command.run', 'filesystem.read', 'network.forward']),
      }),
      expect.objectContaining({
        id: 'terminal-session:term-1',
        kind: 'terminal-session',
        sessionId: 'term-1',
        capabilities: expect.arrayContaining(['terminal.send', 'terminal.observe']),
      }),
      expect.objectContaining({
        id: 'tab:tab-sftp',
        kind: 'sftp-session',
        capabilities: expect.arrayContaining(['filesystem.read', 'filesystem.write']),
      }),
    ]));
  });

  it('builds capability rows scoped to active targets', () => {
    const capabilities = buildCapabilityStatuses([
      createToolTarget({
        id: 'ssh-node:node-1',
        kind: 'ssh-node',
        label: 'root@example.com',
        active: true,
        nodeId: 'node-1',
        capabilities: ['command.run', 'filesystem.read'],
      }),
    ]);

    expect(capabilities).toEqual([
      expect.objectContaining({
        targetId: 'ssh-node:node-1',
        capability: 'command.run',
        notes: 'active target',
      }),
      expect.objectContaining({
        targetId: 'ssh-node:node-1',
        capability: 'filesystem.read',
        notes: 'active target',
      }),
    ]);
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

describe('file safety protocol helpers', () => {
  it('normalizes safe write request aliases', () => {
    expect(parseFileWriteRequest({
      path: ' /tmp/a.txt ',
      content: 'hello',
      expected_hash: 'hash-1',
      expected_mtime: 123,
      create_only: true,
      dry_run: true,
    })).toMatchObject({
      path: '/tmp/a.txt',
      content: 'hello',
      expectedHash: 'hash-1',
      expectedMtime: 123,
      createOnly: true,
      dryRun: true,
    });
  });

  it('builds lightweight diff summaries without retaining extra copies', () => {
    const summary = buildFileDiffSummary({
      beforeContent: 'old',
      beforeHash: 'hash-old',
      afterContent: 'new',
      afterHash: 'hash-new',
    });

    expect(summary).toEqual({
      beforeSize: byteLengthOfText('old'),
      afterSize: byteLengthOfText('new'),
      beforeHash: 'hash-old',
      afterHash: 'hash-new',
      changed: true,
    });
  });
});
