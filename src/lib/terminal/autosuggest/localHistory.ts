// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

const MAX_HISTORY_BYTES = 512 * 1024;
const MAX_COMMANDS = 500;

export async function loadLocalShellHistoryCommands(): Promise<string[]> {
  try {
    const [{ homeDir }, { readTextFile }] = await Promise.all([
      import('@tauri-apps/api/path'),
      import('@tauri-apps/plugin-fs'),
    ]);
    const home = await homeDir();
    const homePrefix = home.endsWith('/') ? home : `${home}/`;
    const files = ['.zsh_history', '.bash_history', '.zhistory', '.local/share/fish/fish_history'];
    const commands: string[] = [];

    for (const file of files) {
      try {
        const content = await readTextFile(`${homePrefix}${file}`);
        commands.push(...parseHistoryFile(file, content.slice(-MAX_HISTORY_BYTES)));
      } catch {
        // Missing history files are expected.
      }
    }

    return commands.slice(-MAX_COMMANDS);
  } catch {
    return [];
  }
}

function parseHistoryFile(path: string, content: string): string[] {
  if (path.includes('fish_history')) {
    return content
      .split('\n')
      .filter((line) => line.startsWith('- cmd: '))
      .map((line) => line.slice('- cmd: '.length).replace(/\\n/g, '\n').trim())
      .filter(Boolean);
  }

  return content
    .split('\n')
    .map((line) => {
      const zshExtended = line.match(/^: \d+:\d+;(.*)$/);
      return (zshExtended ? zshExtended[1] : line).trim();
    })
    .filter(Boolean);
}
