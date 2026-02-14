import type { Terminal } from '@xterm/xterm';
import { useSettingsStore } from '../store/settingsStore';

type Disposable = { dispose: () => void };
type LoadableAddon = Disposable & { activate: (terminal: Terminal) => void };

type ClipboardProvider = {
  readText: (selection: string) => Promise<string>;
  writeText: (selection: string, text: string) => Promise<void>;
};

type Base64Codec = {
  encodeText: (data: string) => string;
  decodeText: (data: string) => string;
};

const MAX_OSC52_BASE64_LENGTH = 1_048_576;

function decodeBase64Utf8(payload: string): string {
  const bytes = Uint8Array.from(atob(payload), (c) => c.charCodeAt(0));
  return new TextDecoder('utf-8').decode(bytes);
}

function encodeUtf8Base64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  return btoa(binary);
}

function isOsc52Enabled(): boolean {
  return useSettingsStore.getState().settings.terminal.osc52Clipboard;
}

function createSecureClipboardAdapter(): ClipboardProvider & Base64Codec {
  return {
    async readText(selection: string): Promise<string> {
      // Keep existing security posture: deny OSC 52 clipboard read requests.
      void selection;
      return '';
    },
    async writeText(selection: string, text: string): Promise<void> {
      if (
        selection !== 'c' ||
        !text ||
        !isOsc52Enabled() ||
        !navigator.clipboard?.writeText
      ) {
        return;
      }
      await navigator.clipboard.writeText(text);
    },
    encodeText(data: string): string {
      return encodeUtf8Base64(data);
    },
    decodeText(data: string): string {
      if (data.length > MAX_OSC52_BASE64_LENGTH) {
        console.warn('[OSC 52] Payload too large, ignored');
        return '';
      }
      try {
        return decodeBase64Utf8(data);
      } catch {
        console.warn('[OSC 52] Invalid base64 payload');
        return '';
      }
    },
  };
}

function installOsc52Fallback(term: Terminal): Disposable {
  const disposable = term.parser.registerOscHandler(52, (data: string) => {
    if (!isOsc52Enabled()) return true;

    const semicolonIdx = data.indexOf(';');
    if (semicolonIdx === -1) return true;

    const payload = data.slice(semicolonIdx + 1);
    if (!payload || payload === '?') return true;

    if (payload.length > MAX_OSC52_BASE64_LENGTH) {
      console.warn('[OSC 52] Payload too large, ignored');
      return true;
    }

    try {
      const text = decodeBase64Utf8(payload);
      navigator.clipboard.writeText(text).catch((err) => {
        console.warn('[OSC 52] Clipboard write failed:', err);
      });
    } catch {
      console.warn('[OSC 52] Invalid base64 payload');
    }
    return true;
  });

  return { dispose: () => disposable.dispose() };
}

export async function installTerminalClipboardSupport(term: Terminal): Promise<Disposable> {
  const adapter = createSecureClipboardAdapter();
  try {
    const mod = (await import('@xterm/addon-clipboard')) as {
      ClipboardAddon?: new (...args: unknown[]) => LoadableAddon;
    };
    const ClipboardAddon = mod.ClipboardAddon;
    if (!ClipboardAddon) {
      return installOsc52Fallback(term);
    }

    // Constructor: new ClipboardAddon(base64, provider)
    // adapter implements both IBase64 and IClipboardProvider
    const addon = new ClipboardAddon(adapter, adapter);
    term.loadAddon(addon);
    return addon;
  } catch {
    return installOsc52Fallback(term);
  }
}
