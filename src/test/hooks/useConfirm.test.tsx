import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useConfirm } from '@/hooks/useConfirm';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key,
  }),
}));

function ConfirmHarness({ onResolved }: { onResolved: (value: boolean) => void }) {
  const { confirm, ConfirmDialog } = useConfirm();

  return (
    <>
      <button
        onClick={() => {
          void confirm({
            title: 'Delete connection?',
            confirmLabel: 'Delete',
            variant: 'danger',
          }).then(onResolved);
        }}
      >
        open-confirm
      </button>
      <button>after-confirm</button>
      {ConfirmDialog}
    </>
  );
}

describe('useConfirm', () => {
  it('releases the dialog pointer lock after confirming', async () => {
    const onResolved = vi.fn();

    render(<ConfirmHarness onResolved={onResolved} />);

    fireEvent.click(screen.getByText('open-confirm'));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(onResolved).toHaveBeenCalledWith(true);
    });
    await waitFor(() => {
      expect(document.body.style.pointerEvents).not.toBe('none');
    });
  });
});
