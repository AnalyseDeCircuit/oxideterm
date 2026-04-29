import { describe, expect, it, vi } from 'vitest';
import type { AiTarget } from '@/lib/ai/orchestrator';

const nodeIdeExecCommandMock = vi.hoisted(() => vi.fn());
const localExecCommandMock = vi.hoisted(() => vi.fn());

vi.mock('@/lib/api', () => ({
  nodeIdeExecCommand: nodeIdeExecCommandMock,
  api: {
    localExecCommand: localExecCommandMock,
  },
}));

vi.mock('@/lib/terminalRegistry', () => ({
  findPaneBySessionId: vi.fn(),
  getTerminalBuffer: vi.fn(),
  readScreen: vi.fn(),
  subscribeTerminalOutput: vi.fn(),
  waitForTerminalReady: vi.fn(),
  writeToTerminal: vi.fn(),
}));

import { runCommandOnTarget } from '@/lib/ai/capabilities/terminal';

const sshTarget: AiTarget = {
  id: 'ssh-node:node-1',
  kind: 'ssh-node',
  label: 'node-1',
  state: 'connected',
  capabilities: ['command.run'],
  refs: { nodeId: 'node-1' },
};

const localTarget: AiTarget = {
  id: 'local-shell:default',
  kind: 'local-shell',
  label: 'Local shell',
  state: 'available',
  capabilities: ['command.run'],
  refs: {},
};

describe('orchestrator terminal command execution', () => {
  it('treats remote null exit code with captured output as successful observation', async () => {
    nodeIdeExecCommandMock.mockResolvedValueOnce({
      stdout: 'file-a\nfile-b\n',
      stderr: '',
      exitCode: null,
    });

    const result = await runCommandOnTarget({ target: sshTarget, command: 'ls -la' });

    expect(result.ok).toBe(true);
    expect(result.error).toBeUndefined();
    expect(result.summary).toContain('exit code was not reported');
    expect(result.observations?.[0]).toContain('did not report an exit code');
  });

  it('keeps non-zero remote exit code as a failed command', async () => {
    nodeIdeExecCommandMock.mockResolvedValueOnce({
      stdout: '',
      stderr: 'Permission denied',
      exitCode: 1,
    });

    const result = await runCommandOnTarget({ target: sshTarget, command: 'cat /root/secret' });

    expect(result.ok).toBe(false);
    expect(result.error).toMatchObject({
      code: 'remote_command_failed',
      recoverable: true,
    });
  });

  it('treats local null exit code with captured output as successful when not timed out', async () => {
    localExecCommandMock.mockResolvedValueOnce({
      stdout: 'hello\n',
      stderr: '',
      exitCode: null,
      timedOut: false,
    });

    const result = await runCommandOnTarget({ target: localTarget, command: 'echo hello' });

    expect(result.ok).toBe(true);
    expect(result.error).toBeUndefined();
    expect(result.observations?.[0]).toContain('did not report an exit code');
  });
});
