# Current Directory Awareness

Current-directory awareness is intentionally disabled by default behind the
hidden `terminal.commandBar.currentDirectoryAwareness` setting.

The code stays in this module because the feature is still the right long-term
direction, but the current signal is not trustworthy enough to be user-facing by
default:

- User-typed `cd` commands are not a closed-loop fact source. Failed commands,
  quoted shell words, aliases, functions, and shell-specific path expansion can
  all make the terminal-side model diverge from the real shell cwd.
- Prompt text, tab titles, host labels, and SSH node names are presentation data.
  They must not be used as cwd ownership or directory-state evidence.
- SSH cwd belongs to the active terminal session/channel. It must be updated from
  terminal-owned facts, not inferred from the reused node transport.
- Directory pickers and project probes must not inject hidden shell commands just
  to discover cwd. Hidden commands are surprising and can interfere with TUI or
  partially typed shell state.

Re-enable this as a visible feature only after there is a reliable event source,
such as shell integration / OSC 7 / completed-command state that can report cwd
as an explicit terminal event. That implementation must preserve active-pane
ownership, work for local and SSH terminals, and avoid prompt or host-title
inference.
