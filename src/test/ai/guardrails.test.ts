import { describe, expect, it } from 'vitest';

import { detectPseudoToolTranscript, shouldTriggerHardDeny } from '@/lib/ai/turnModel/guardrails';

describe('detectPseudoToolTranscript', () => {
  it('detects tool-like JSON when no structured tool call was seen', () => {
    const result = detectPseudoToolTranscript({
      toolUseEnabled: false,
      sawStructuredToolCall: false,
      assistantText: '{"name":"terminal_exec","arguments":{"command":"pwd"}}',
    });

    expect(result.matched).toBe(true);
    expect(result.reason).toBe('pseudo-tool-transcript');
    expect(shouldTriggerHardDeny({
      toolUseEnabled: false,
      sawStructuredToolCall: false,
      assistantText: '{"name":"terminal_exec","arguments":{"command":"pwd"}}',
    }, result)).toBe(true);
  });

  it('does not detect when the user explicitly requested JSON', () => {
    const result = detectPseudoToolTranscript({
      toolUseEnabled: false,
      sawStructuredToolCall: false,
      assistantText: '{"name":"terminal_exec","arguments":{"command":"pwd"}}',
      userExplicitlyRequestedJson: true,
    });

    expect(result).toEqual({ matched: false });
  });

  it('does not detect when a structured tool call already exists', () => {
    const result = detectPseudoToolTranscript({
      toolUseEnabled: true,
      sawStructuredToolCall: true,
      assistantText: '{"name":"terminal_exec","arguments":{"command":"pwd"}}',
    });

    expect(result).toEqual({ matched: false });
  });
});