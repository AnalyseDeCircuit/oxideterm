import { describe, expect, it } from 'vitest';
import { buildToolOperationStrategyPrompt, buildTuiInteractionGuidelines } from '@/lib/ai/toolUsePrompt';

describe('toolUsePrompt', () => {
  it('describes target discovery, command routing, terminal interaction, and safe writes', () => {
    const prompt = buildToolOperationStrategyPrompt();

    expect(prompt).toContain('First identify the task type');
    expect(prompt).toContain('resolve the target with `resolve_target`');
    expect(prompt).toContain('Treat the current UI/tab as a hint only');
    expect(prompt).toContain('`terminal_exec` + `target_id` for an `ssh-node`');
    expect(prompt).toContain('`target_id` for the `terminal-session`');
    expect(prompt).toContain('Use observe-send-observe');
    expect(prompt).toContain('do not repeat the command and do not guess credentials');
    expect(prompt).toContain('pass `expectedHash`');
    expect(prompt).toContain('verify by reading the file back');
  });

  it('adds local terminal focus rules only for local terminal tabs', () => {
    expect(buildToolOperationStrategyPrompt()).not.toContain('Local Terminal Focus');
    expect(buildToolOperationStrategyPrompt({ activeTabType: 'local_terminal' })).toContain('prefer `local_exec`');
  });

  it('keeps TUI interaction guidance in a reusable prompt section', () => {
    const prompt = buildTuiInteractionGuidelines();

    expect(prompt).toContain('Call `read_screen` first');
    expect(prompt).toContain('After `send_keys`, call `read_screen`');
    expect(prompt).toContain('Check `isAlternateBuffer` first');
  });
});
