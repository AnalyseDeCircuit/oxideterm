import { describe, expect, it } from 'vitest';
import {
  normalizeFocusHandoffCommands,
  parseFocusHandoffCommandList,
  shouldHandOffFocusToTerminal,
} from '@/lib/terminal/focusHandoff';

describe('focusHandoff', () => {
  it('normalizes user configured command lists', () => {
    expect(parseFocusHandoffCommandList('vim, nvim\nlazygit vim bad command')).toEqual([
      'vim',
      'nvim',
      'lazygit',
      'bad',
      'command',
    ]);
    expect(normalizeFocusHandoffCommands(['VIM', 'vim', 'my-tui', 'bad/token'])).toEqual(['vim', 'my-tui']);
  });

  it('hands off focus only for configured interactive commands', () => {
    expect(shouldHandOffFocusToTerminal('ls -la', ['vim'])).toBe(false);
    expect(shouldHandOffFocusToTerminal('sudo -E vim /tmp/a.txt', ['vim'])).toBe(true);
    expect(shouldHandOffFocusToTerminal('env TERM=xterm-256color lazygit', ['lazygit'])).toBe(true);
  });
});
