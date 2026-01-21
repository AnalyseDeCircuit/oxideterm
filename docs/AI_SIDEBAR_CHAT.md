# AI Sidebar Chat

> OxideTerm's intelligent terminal assistant with persistent conversations

## Overview

The AI Sidebar Chat provides an integrated AI assistant directly in the OxideTerm sidebar. Unlike the quick inline AI panel, the sidebar chat maintains persistent conversation history, allowing for continuous context across multiple interactions.

| Feature | Description |
|---------|-------------|
| **Persistent History** | Conversations are saved to localStorage and survive app restarts |
| **Streaming Responses** | Real-time streaming responses with stop capability |
| **Terminal Context** | Optionally include terminal buffer content for context-aware assistance |
| **Code Execution** | Insert AI-generated commands directly into active terminal |
| **Multi-language** | Full i18n support across 9 languages |

## Features

### 💬 Conversation Management

- **Multiple Conversations**: Create and manage separate conversation threads
- **Auto-titles**: Conversations are automatically titled based on the first message
- **Quick Delete**: Remove individual conversations or clear all history
- **Conversation Switching**: Seamlessly switch between past conversations

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

### State Management

The AI chat uses a Zustand store (`aiChatStore.ts`) for state management:

```typescript
interface AiChatState {
  conversations: AiConversation[];
  activeConversationId: string | null;
  isLoading: boolean;
  error: string | null;
  abortController: AbortController | null;
}
```

### Components

| Component | Purpose |
|-----------|---------|
| `AiChatPanel.tsx` | Main panel with conversation management |
| `ChatMessage.tsx` | Message rendering with code block support |
| `ChatInput.tsx` | Input area with context toggle |

### Data Flow

```
User Input
    ↓
ChatInput (context capture optional)
    ↓
aiChatStore.sendMessage()
    ↓
streamChatCompletion() (OpenAI API)
    ↓
Streaming response → ChatMessage render
    ↓
Command insertion (optional) → Active terminal
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Enter` | Send message |
| `Shift+Enter` | New line in input |

## Localization

Full i18n support is available in:

- 🇺🇸 English
- 🇨🇳 中文 (Simplified Chinese)
- 🇯🇵 日本語 (Japanese)
- 🇰🇷 한국어 (Korean)
- 🇩🇪 Deutsch (German)
- 🇫🇷 Français (French)
- 🇪🇸 Español (Spanish)
- 🇧🇷 Português (Brazilian Portuguese)
- 🇻🇳 Tiếng Việt (Vietnamese)

## Technical Notes

### Terminal Registry

The `terminalRegistry.ts` module provides a robust mechanism for AI context capture:

```typescript
interface TerminalEntry {
  getter: BufferGetter;
  registeredAt: number;
  tabId: string;
}
```

**Safety Features:**
- **Tab ID Validation**: Each registry entry is bound to a specific tab ID, preventing cross-tab context leakage
- **Expiration Check**: Entries older than 5 minutes are automatically invalidated
- **Error Isolation**: Failed getter calls are caught and return null gracefully

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
