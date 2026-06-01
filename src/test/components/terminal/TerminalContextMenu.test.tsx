import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TerminalContextMenu } from '@/components/terminal/TerminalContextMenu';

describe('TerminalContextMenu', () => {
  it('runs copy and paste actions from the app-owned terminal menu', () => {
    const onCopy = vi.fn();
    const onPaste = vi.fn();
    const onClose = vi.fn();

    render(
      <TerminalContextMenu
        menu={{ x: 32, y: 48, canCopy: true }}
        copyLabel="Copy"
        pasteLabel="Paste"
        onCopy={onCopy}
        onPaste={onPaste}
        onClose={onClose}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    fireEvent.click(screen.getByRole('button', { name: 'Paste' }));

    expect(onCopy).toHaveBeenCalledTimes(1);
    expect(onPaste).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('disables copy when the terminal has no selection', () => {
    render(
      <TerminalContextMenu
        menu={{ x: 32, y: 48, canCopy: false }}
        copyLabel="Copy"
        pasteLabel="Paste"
        onCopy={vi.fn()}
        onPaste={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Copy' })).toBeDisabled();
  });
});
