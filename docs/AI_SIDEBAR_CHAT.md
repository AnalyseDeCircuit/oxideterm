# AI Sidebar Chat

> OxideTerm's intelligent terminal assistant with persistent conversations and deep context awareness

## Overview

The AI Sidebar Chat provides an integrated AI assistant directly in the OxideTerm sidebar. Unlike the quick inline AI panel, the sidebar chat maintains persistent conversation history, allowing for continuous context across multiple interactions.

| Feature | Description |
|---------|-------------|
| **Persistent History** | Conversations are saved to redb database and survive app restarts |
| **Streaming Responses** | Real-time streaming responses with stop capability |
| **Auto Context Injection** | Automatically captures environment, buffer, and selection context |
| **Terminal Context** | Optionally include terminal buffer content for context-aware assistance |
| **Code Execution** | Insert AI-generated commands directly into active terminal |
| **Multi-language** | Full i18n support across 11 languages |

## Features

### 💬 Conversation Management

- **Multiple Conversations**: Create and manage separate conversation threads
- **Auto-titles**: Conversations are automatically titled based on the first message
- **Quick Delete**: Remove individual conversations or clear all history
- **Conversation Switching**: Seamlessly switch between past conversations

### 🧠 Automatic Context Injection (NEW in v1.4.1)

The sidebar chat now automatically gathers deep context from your environment—similar to GitHub Copilot's awareness:

#### 1. Environment Snapshot
When you send a message, the AI automatically knows:
- **Local OS**: macOS / Windows / Linux
- **Terminal Type**: SSH or Local terminal
- **Connection Details**: `user@host` for SSH sessions
- **Remote OS Hint**: Guessed from hostname patterns

#### 2. Dynamic Buffer Sync
The last 50 lines of terminal output are automatically included as context, giving the AI visibility into:
- Recent command outputs
- Error messages
- System responses

#### 3. Selection Priority
If you have text selected in the terminal, it becomes the **primary focus**:
- Selection is marked as "Focus Area" in the context
- AI treats selected text as the main subject of your query
- Perfect for asking about specific error messages or log lines

### 🖥️ Terminal Integration

- **Context Capture**: Click "Include context" to attach terminal buffer content to your message
- **Command Insertion**: Click the ▶️ button on code blocks to insert commands into the active terminal
- **Multiline Support**: Multi-line commands are inserted using bracketed paste mode for proper handling

### 📝 Message Rendering

- **Markdown Support**: Inline code and code blocks are properly formatted
- **Syntax Detection**: Shell/bash/zsh/powershell code blocks show an insert button
- **Copy to Clipboard**: Quick copy button on all code blocks

### ⚡ Quick Prompts

When starting a new conversation, quick prompt buttons are available:

- **Explain a command** - Get help understanding shell commands
- **Find files matching...** - Learn file search techniques
- **Write a shell script** - Generate custom scripts
- **Optimize this command** - Improve command efficiency

## Configuration

AI Chat uses the same settings as the inline AI assistant. Configure in **Settings > AI**:

| Setting | Description |
|---------|-------------|
| `ai.enabled` | Enable/disable AI features |
| `ai.apiEndpoint` | OpenAI-compatible API endpoint |
| `ai.apiKey` | Your API key |
| `ai.model` | Model to use (e.g., `gpt-4o-mini`) |
| `ai.contextVisibleLines` | Number of terminal lines to capture for context |

## Architecture

### Persistence Layer (redb Backend)

AI conversations are persisted using a dedicated redb database (`chat_history.redb`) in the config directory:

```
~/.config/oxideterm/
├── state.redb            # Sessions, forwards, settings
└── chat_history.redb     # AI conversations (NEW)
```

**Database Schema:**

| Table | Key | Value |
|-------|-----|-------|
| `conversations` | conversation_id (string) | ConversationMeta (msgpack) |
| `messages` | message_id (string) | PersistedMessage (msgpack) |
| `conversation_messages` | conversation_id | Vec<message_id> (msgpack) |
| `ai_chat_metadata` | key | value |

**Data Types:**

```rust
struct PersistedMessage {
    id: String,
    conversation_id: String,
    role: MessageRole,        // "user" | "assistant"
    content: String,
    timestamp: u64,           // Unix millis
    context_snapshot: Option<ContextSnapshot>,
}

struct ContextSnapshot {
    session_id: Option<String>,
    connection_name: Option<String>,
    remote_os: Option<String>,
    cwd: Option<String>,
    selection: Option<String>,
    buffer_tail: Option<String>,  // zstd compressed if >4KB
    buffer_compressed: bool,
}
```

**Features:**
- **zstd Compression**: Buffer snapshots >4KB are automatically compressed
- **LRU Eviction**: Max 100 conversations, oldest auto-deleted
- **Message Limits**: Max 200 messages per conversation
- **Lazy Loading**: Only conversation list loaded initially, messages loaded on demand

### State Management

The AI chat uses a Zustand store (`aiChatStore.ts`) for state management:

```typescript
interface AiChatState {
  conversations: AiConversation[];
  activeConversationId: string | null;
  isLoading: boolean;
  isInitialized: boolean;  // NEW: Backend sync status
  error: string | null;
  abortController: AbortController | null;
}
```

### Context Injection Pipeline

The new `sidebarContextProvider.ts` module aggregates context automatically:

```typescript
// Gather complete sidebar context for AI
const context = gatherSidebarContext({
  maxBufferLines: 50,      // Last 50 lines from terminal
  maxBufferChars: 8000,    // Max 8KB of buffer content
  maxSelectionChars: 2000, // Max 2KB of selection
});

// Context structure
interface SidebarContext {
  env: EnvironmentSnapshot;     // OS, connection, session info
  terminal: TerminalContext;    // Buffer and selection
  systemPromptSegment: string;  // Formatted for system prompt
  contextBlock: string;         // Formatted for API context
}
```

### Data Flow

```
User Input
    ↓
ChatInput (context capture optional)
    ↓
aiChatStore.sendMessage()
    ↓
gatherSidebarContext() ← Auto-inject environment snapshot
    ├── Local OS detection (platform.ts)
    ├── Connection details (appStore/sessionTreeStore)
    ├── Buffer content (terminalRegistry)
    └── Selection text (terminalRegistry)
    ↓
Enhanced System Prompt + Context Block
    ↓
streamChatCompletion() (OpenAI API)
    ↓
Streaming response → ChatMessage render
    ↓
Command insertion (optional) → Active terminal
```

### Components

| Component | Purpose |
|-----------|---------|
| `AiChatPanel.tsx` | Main panel with conversation management |
| `ChatMessage.tsx` | Message rendering with code block support |
| `ChatInput.tsx` | Input area with context toggle |
| `sidebarContextProvider.ts` | Environment and terminal context aggregation |

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Enter` | Send message |
| `Shift+Enter` | New line in input |

## Localization

Full i18n support is available in:

- 🇺🇸 English
- 🇨🇳 中文 (Simplified Chinese)
-  繁體中文 (Traditional Chinese)
- 🇯🇵 日本語 (Japanese)
- 🇰🇷 한국어 (Korean)
- 🇩🇪 Deutsch (German)
- 🇫🇷 Français (French)
- 🇪🇸 Español (Spanish)
- 🇮🇹 Italiano (Italian)
- 🇧🇷 Português (Brazilian Portuguese)
- 🇻🇳 Tiếng Việt (Vietnamese)

## Technical Notes

### Terminal Registry with Selection Support

The `terminalRegistry.ts` module provides robust mechanisms for AI context capture:

```typescript
interface TerminalEntry {
  getter: BufferGetter;           // Get buffer content
  selectionGetter?: SelectionGetter; // Get current selection (NEW)
  registeredAt: number;
  tabId: string;
  sessionId: string;
  terminalType: 'terminal' | 'local_terminal';
}

// New selection APIs
export function getActiveTerminalSelection(): string | null;
export function getTerminalSelection(paneId: string): string | null;
```

**Safety Features:**
- **Tab ID Validation**: Each registry entry is bound to a specific tab ID, preventing cross-tab context leakage
- **Expiration Check**: Entries older than 5 minutes are automatically invalidated
- **Error Isolation**: Failed getter calls are caught and return null gracefully
- **Selection Isolation**: Selection getters are optional and fail gracefully

### Sidebar Context Provider

The new `sidebarContextProvider.ts` module provides:

```typescript
// Main API
export function gatherSidebarContext(config): SidebarContext;
export function getEnvironmentInfo(): EnvironmentSnapshot;  // Lightweight
export function hasTerminalContext(): boolean;              // Quick check
export function getQuickSelection(): string | null;         // Selection only

// Environment detection
function getLocalOS(): 'macOS' | 'Windows' | 'Linux';
function guessRemoteOS(host, username): string | null;
```

**Context Format in System Prompt:**
```
## Environment
- Local OS: macOS
- Terminal: SSH to user@example.com
- Remote OS: Linux (guessed)

## User Selection (Priority Focus)
The user has selected specific text in the terminal...
```

**Context Format in API Messages:**
```
=== SELECTED TEXT (Focus Area) ===
[selected text here]

=== Terminal Output (last 50 lines) ===
[buffer content here]
```

### Bracketed Paste Mode

When inserting multi-line commands, the system uses bracketed paste mode escape sequences (`\x1b[200~...\x1b[201~`) to ensure the entire command block is treated as a single paste operation by the shell.

### Empty Message Handling

The system automatically filters out empty assistant messages when building API requests to avoid validation errors from the OpenAI API.

### Scroll Buffer API

Terminal context capture uses different methods depending on terminal type:

- **SSH terminals**: Uses the `getScrollBuffer` Tauri command to retrieve scroll buffer from the Rust backend
- **Local terminals**: Uses the Terminal Registry pattern with xterm.js buffer API for synchronous access

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Enable AI in Settings first" | Go to Settings > AI and enable AI features |
| No response from AI | Check API endpoint and key configuration |
| Context not captured | Ensure you have an active terminal tab (SSH or local) |
| Insert button not showing | Only shell/bash/zsh/powershell code blocks show insert button |
| Selection not detected | Make sure terminal has focus before selecting text |

---

*Documentation version: v1.4.1 | Last updated: 2026-02-05*
