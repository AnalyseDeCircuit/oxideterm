import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('lucide-react', () => ({
  Eye: () => <span>eye</span>,
  EyeOff: () => <span>eye-off</span>,
  Loader2: () => <span>loading</span>,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ open, children }: { open: boolean; children: React.ReactNode }) => open ? <div>{children}</div> : null,
  DialogContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogDescription: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogFooter: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogTitle: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => <button {...props}>{children}</button>,
}));

vi.mock('@/components/ui/input', () => ({
  Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => <input {...props} />,
}));

vi.mock('@/components/ui/label', () => ({
  Label: ({ children, ...props }: React.LabelHTMLAttributes<HTMLLabelElement>) => <label {...props}>{children}</label>,
}));

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectValue: () => null,
}));

vi.mock('@/components/ui/tabs', () => ({
  Tabs: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TabsList: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  TabsTrigger: ({ children }: { children: React.ReactNode }) => <button type="button">{children}</button>,
  TabsContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

const api = vi.hoisted(() => ({
  getGroups: vi.fn().mockResolvedValue([]),
  getConnectionPassword: vi.fn().mockResolvedValue('saved-secret'),
  saveConnection: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@/lib/api', () => ({ api }));

import { EditConnectionPropertiesModal } from '@/components/modals/EditConnectionPropertiesModal';

describe('EditConnectionPropertiesModal', () => {
  beforeEach(() => {
    api.getGroups.mockReset();
    api.getGroups.mockResolvedValue([]);
    api.getConnectionPassword.mockReset();
    api.getConnectionPassword.mockResolvedValue('saved-secret');
    api.saveConnection.mockReset();
    api.saveConnection.mockResolvedValue(undefined);
  });

  it('loads the saved password lazily and submits updates', async () => {
    render(
      <EditConnectionPropertiesModal
        open={true}
        onOpenChange={vi.fn()}
        connection={{
          id: 'conn-1',
          name: 'Saved Password Host',
          host: 'example.com',
          port: 22,
          username: 'tester',
          auth_type: 'password',
        } as never}
      />,
    );

    const revealButton = screen.getByRole('button', { name: 'sessionManager.edit_properties.show_password' });
    fireEvent.click(revealButton);

    await waitFor(() => {
      expect(api.getConnectionPassword).toHaveBeenCalledWith('conn-1');
    });

    await waitFor(() => {
      const passwordInput = screen.getByLabelText('sessionManager.edit_properties.saved_password') as HTMLInputElement;
      expect(passwordInput.value).toBe('saved-secret');
    });

    const passwordInput = screen.getByLabelText('sessionManager.edit_properties.saved_password') as HTMLInputElement;

    fireEvent.change(passwordInput, { target: { value: 'updated-secret' } });
    fireEvent.click(screen.getByRole('button', { name: 'sessionManager.edit_properties.save' }));

    await waitFor(() => {
      expect(api.saveConnection).toHaveBeenCalledWith(expect.objectContaining({
        id: 'conn-1',
        auth_type: 'password',
        password: 'updated-secret',
      }));
    });
  });

  it('shows an inline error when loading the saved password fails', async () => {
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    api.getConnectionPassword.mockRejectedValueOnce(new Error('Keychain unavailable'));

    try {
      render(
        <EditConnectionPropertiesModal
          open={true}
          onOpenChange={vi.fn()}
          connection={{
            id: 'conn-2',
            name: 'Broken Password Host',
            host: 'broken.example.com',
            port: 22,
            username: 'tester',
            auth_type: 'password',
          } as never}
        />,
      );

      fireEvent.click(screen.getByRole('button', { name: 'sessionManager.edit_properties.show_password' }));

      await waitFor(() => {
        expect(screen.getByText('Keychain unavailable')).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText('sessionManager.edit_properties.saved_password') as HTMLInputElement;
      expect(passwordInput.value).toBe('');
      expect(api.saveConnection).not.toHaveBeenCalled();
    } finally {
      consoleErrorSpy.mockRestore();
    }
  });
});