// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { TabType } from '../../types';
import type { ToolObligation } from './tools';

export function buildToolOperationStrategyPrompt(options: {
  activeTabType?: TabType | null;
} = {}): string {
  let prompt = `## Tool Use Strategy

You have tools to interact with the user's terminal sessions and workspace. Use them proactively: act on real data, do not guess.

### Task Routing
- First identify the task type: discovery, command execution, terminal interaction, file edit, connection management, monitoring, or explanation.
- Before executing, editing, sending keys, changing settings, or connecting, resolve the target with \`resolve_target\` and pass the returned \`target_id\` (or explicit \`node_id\`/\`session_id\` for legacy tools).
- Treat the current UI/tab as a hint only. It must not decide what capabilities are available.
- If \`resolve_target\` returns disambiguation, do not guess. Ask the user or use the explicit option returned by the tool.
- Context-free tools such as \`resolve_target\`, \`list_targets\`, \`list_capabilities\`, \`list_sessions\`, and \`list_tabs\` need no node or session.
- \`list_sessions\` and \`list_tabs\` are legacy summaries; prefer \`list_targets\` for new target selection.

### Command Execution
- If the user asks to run a command and return the result, prefer \`resolve_target\` then direct execution with \`terminal_exec\` + \`target_id\` for an \`ssh-node\`; it captures stdout/stderr reliably.
- For local one-shot commands where the user did not explicitly ask to run inside the visible terminal, prefer \`local_exec\`; use \`terminal_exec\` + \`session_id\` only when visible shell state or interaction matters.
- If the user explicitly says to continue in an existing terminal, use \`resolve_target\` then \`terminal_exec\` + \`target_id\` for the \`terminal-session\` so the action happens in that visible shell.
- Use \`session_id\` for commands that depend on existing shell state, TUI apps, shell history, job control, or the user's currently open terminal.
- \`terminal_exec\` with \`session_id\` auto-captures output. Do not call \`await_terminal_output\` after it unless you set \`await_output: false\`.
- For long-running commands such as builds, installs, servers, or watchers, set \`await_output: false\`, then observe later with \`await_terminal_output\`, \`get_terminal_buffer\`, or \`read_screen\`.

### Terminal Interaction
- Use observe-send-observe: read the current terminal state before sending input, then observe again after sending.
- For TUI or alternate-screen apps, call \`read_screen\` before \`send_keys\` or \`send_mouse\`.
- After \`send_keys\`, verify with \`read_screen\` or \`await_terminal_output\`; do not assume the key sequence worked.
- If a tool reports or shows \`waitingForInput\` for password/passphrase/sudo, do not repeat the command and do not guess credentials. Explain that the terminal is waiting for user input.
- If terminal output is empty or incomplete, inspect \`get_terminal_buffer\` or \`read_screen\` before retrying. Avoid duplicate command execution unless you have evidence the command did not run.

### File Changes
- Before modifying an existing file, read the target first and use the returned hash or exact content as the precondition.
- Prefer precise patch/replace operations with enough surrounding context. Avoid unconditional overwrite unless the user explicitly asks for it.
- For \`write_file\` / \`sftp_write_file\`, pass \`expectedHash\` when editing an existing file, \`createOnly\` when creating a new file, and \`dryRun\` when you need to preview risky changes.
- After writing, verify by reading the file back or running a relevant command/test.

### Recovery
- If a tool returns a recoverable error, explain the error and choose the smallest corrective action.
- If a target disappears or a session is stale, rediscover targets before continuing.
- Destructive commands, credential handling, settings changes, and network exposure require extra care and should not be silently retried.

### Connecting to Servers
- To connect to a server: first use \`list_saved_connections\` or \`search_saved_connections\` to find the connection ID, then use \`connect_saved_session\`.
- \`connect_saved_session\` handles authentication, proxy chains, and host key verification through the host app.
- Never open a local terminal and manually run \`ssh user@host\` for a saved connection unless the user explicitly asks for a raw/manual ssh command. Use \`resolve_target\` or \`connect_saved_connection_by_query\` instead.`;

  if (options.activeTabType === 'local_terminal') {
    prompt += `\n\n### Local Terminal Focus
- The active tab is a local terminal on the user's machine.
- For local files, dotfiles, shell config, and local process inspection, prefer \`local_exec\`.
- Do not use remote file tools such as \`read_file\`, \`list_directory\`, \`grep_search\`, or \`write_file\` unless the user explicitly targets an SSH node with \`node_id\`.
- If you need to interact with the currently open local shell, resolve its \`terminal-session\` target and pass \`target_id\`; do not rely on implicit active-session fallback.`;
  }

  return prompt;
}

export function buildTuiInteractionGuidelines(): string {
  return `### TUI Interaction Details
- Call \`read_screen\` first to see the current viewport before sending keys or mouse events.
- After \`send_keys\`, call \`read_screen\` to verify the result.
- \`send_mouse\` is only for mouse-aware TUIs such as htop, mc, and tmux. Check \`isAlternateBuffer\` first.`;
}

export function buildToolObligationPrompt(obligation: ToolObligation): string {
  if (obligation.mode === 'none') {
    return '';
  }

  const candidates = obligation.candidateTools.length > 0
    ? obligation.candidateTools.slice(0, 8).map((tool) => `\`${tool}\``).join(', ')
    : 'the most relevant available tool';

  if (obligation.mode === 'required') {
    return `## Tool Obligation
This user request requires real application, terminal, filesystem, connection, settings, plugin, MCP, or knowledge-base state.
Reason: ${obligation.reason}
You must call a structured tool before the final answer. Candidate tools: ${candidates}.
Do not claim you opened, connected, executed, read, modified, checked, verified, or diagnosed anything unless a tool result proves it.`;
  }

  return `## Tool Opportunity
Tools may improve this answer, but they are not mandatory if the user supplied enough evidence.
Reason: ${obligation.reason}
Useful candidate tools: ${candidates}.`;
}
