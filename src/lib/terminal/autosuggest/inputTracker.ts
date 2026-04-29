// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { TerminalAutosuggestInputState } from './types';

const ESC = '\x1b';

export class TerminalAutosuggestInputTracker {
  private value = '';
  private cursorIndex = 0;
  private dirty = false;

  getState(): TerminalAutosuggestInputState {
    return {
      value: this.value,
      cursorIndex: this.cursorIndex,
      isCursorAtEnd: this.cursorIndex === this.value.length,
    };
  }

  reset(): void {
    this.value = '';
    this.cursorIndex = 0;
    this.dirty = false;
  }

  accept(text: string): void {
    if (!text) return;
    this.insert(text);
    this.cursorIndex = this.value.length;
  }

  applyData(data: string): { completedCommand?: string; changed: boolean } {
    if (!data) return { changed: false };
    const before = this.snapshot();
    this.dirty = false;

    if (data.includes('\x1b[200~') || data.includes('\x1b[201~')) {
      this.reset();
      return { changed: true };
    }

    let completedCommand: string | undefined;
    let index = 0;
    while (index < data.length) {
      const char = data[index];
      if (char === '\r' || char === '\n') {
        const command = this.value.trim();
        completedCommand = command || undefined;
        this.reset();
        index += 1;
        continue;
      }

      if (char === '\x03') {
        this.reset();
        index += 1;
        continue;
      }

      if (char === '\x15') {
        this.value = this.value.slice(this.cursorIndex);
        this.cursorIndex = 0;
        this.dirty = true;
        index += 1;
        continue;
      }

      if (char === '\x0b') {
        this.value = this.value.slice(0, this.cursorIndex);
        this.dirty = true;
        index += 1;
        continue;
      }

      if (char === '\x01') {
        this.cursorIndex = 0;
        this.dirty = true;
        index += 1;
        continue;
      }

      if (char === '\x05') {
        this.cursorIndex = this.value.length;
        this.dirty = true;
        index += 1;
        continue;
      }

      if (char === '\x7f' || char === '\b') {
        this.backspace();
        index += 1;
        continue;
      }

      if (char === ESC) {
        const consumed = this.applyEscapeSequence(data.slice(index));
        if (consumed > 0) {
          index += consumed;
          continue;
        }
        this.reset();
        index += 1;
        continue;
      }

      if (isPrintable(char)) {
        this.insert(char);
      }
      index += 1;
    }

    return { completedCommand, changed: this.snapshot() !== before || this.dirty };
  }

  private snapshot(): string {
    return `${this.value}\0${this.cursorIndex}`;
  }

  private insert(text: string): void {
    this.value = `${this.value.slice(0, this.cursorIndex)}${text}${this.value.slice(this.cursorIndex)}`;
    this.cursorIndex += text.length;
    this.dirty = true;
  }

  private backspace(): void {
    if (this.cursorIndex <= 0) return;
    this.value = `${this.value.slice(0, this.cursorIndex - 1)}${this.value.slice(this.cursorIndex)}`;
    this.cursorIndex -= 1;
    this.dirty = true;
  }

  private applyEscapeSequence(sequence: string): number {
    if (sequence.startsWith('\x1b[D') || sequence.startsWith('\x1bOD')) {
      this.cursorIndex = Math.max(0, this.cursorIndex - 1);
      this.dirty = true;
      return sequence.startsWith('\x1b[D') ? 3 : 3;
    }
    if (sequence.startsWith('\x1b[C') || sequence.startsWith('\x1bOC')) {
      this.cursorIndex = Math.min(this.value.length, this.cursorIndex + 1);
      this.dirty = true;
      return 3;
    }
    if (sequence.startsWith('\x1b[H') || sequence.startsWith('\x1bOH') || sequence.startsWith('\x1b[1~') || sequence.startsWith('\x1b[7~')) {
      this.cursorIndex = 0;
      this.dirty = true;
      return sequence.startsWith('\x1b[1~') || sequence.startsWith('\x1b[7~') ? 4 : 3;
    }
    if (sequence.startsWith('\x1b[F') || sequence.startsWith('\x1bOF') || sequence.startsWith('\x1b[4~') || sequence.startsWith('\x1b[8~')) {
      this.cursorIndex = this.value.length;
      this.dirty = true;
      return sequence.startsWith('\x1b[4~') || sequence.startsWith('\x1b[8~') ? 4 : 3;
    }
    if (sequence.startsWith('\x1b[3~')) {
      if (this.cursorIndex < this.value.length) {
        this.value = `${this.value.slice(0, this.cursorIndex)}${this.value.slice(this.cursorIndex + 1)}`;
        this.dirty = true;
      }
      return 4;
    }
    return 0;
  }
}

function isPrintable(char: string): boolean {
  if (!char) return false;
  const code = char.codePointAt(0) ?? 0;
  return code >= 0x20 && code !== 0x7f;
}
