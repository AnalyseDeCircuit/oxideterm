// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export const DEFAULT_TERMINAL_FOCUS_HANDOFF_COMMANDS = [
  'btop',
  'emacs',
  'fzf',
  'htop',
  'lazydocker',
  'lazygit',
  'less',
  'man',
  'micro',
  'nano',
  'nvim',
  'ranger',
  'screen',
  'ssh',
  'tig',
  'tmux',
  'top',
  'vi',
  'vim',
  'yazi',
] as const;

export function parseFocusHandoffCommandList(input: string): string[] {
  return normalizeFocusHandoffCommands(input.split(/[\s,]+/));
}

export function normalizeFocusHandoffCommands(commands: readonly string[] | undefined): string[] {
  const normalized = new Set<string>();
  for (const command of commands ?? DEFAULT_TERMINAL_FOCUS_HANDOFF_COMMANDS) {
    const token = command.trim().toLowerCase();
    if (/^[a-z0-9._+-]+$/.test(token)) {
      normalized.add(token);
    }
  }
  return [...normalized];
}

export function shouldHandOffFocusToTerminal(command: string, commands: readonly string[] | undefined): boolean {
  const executable = getExecutableToken(command);
  if (!executable) return false;
  // Do not return focus for every Command Bar submit. Normal commands should
  // keep the user in the Command Bar workflow; only user-configured commands
  // that are likely to take over the terminal keyboard/screen get an immediate
  // xterm focus handoff so TUI apps do not require a risky extra click.
  return new Set(normalizeFocusHandoffCommands(commands)).has(executable);
}

function getExecutableToken(command: string): string | null {
  const segment = command.trim().split(/\s*(?:&&|\|\||;)\s*/).find(Boolean);
  if (!segment) return null;
  const tokens = segment.match(/(?:[^\s"'\\]+|"(?:\\.|[^"])*"|'[^']*')+/g) ?? [];
  let index = 0;
  while (index < tokens.length) {
    const token = stripShellTokenQuotes(tokens[index]);
    if (!token || token.startsWith('-') || /^[A-Za-z_][A-Za-z0-9_]*=/.test(token)) {
      index += 1;
      continue;
    }
    if (token === 'sudo' || token === 'command' || token === 'exec') {
      index += 1;
      continue;
    }
    if (token === 'env') {
      index += 1;
      continue;
    }
    return token.split('/').pop()?.toLowerCase() ?? null;
  }
  return null;
}

function stripShellTokenQuotes(token: string): string {
  if ((token.startsWith('"') && token.endsWith('"')) || (token.startsWith("'") && token.endsWith("'"))) {
    return token.slice(1, -1);
  }
  return token;
}
