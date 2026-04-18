import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const setupPortableKeystoreMock = vi.hoisted(() => vi.fn());
const unlockPortableKeystoreMock = vi.hoisted(() => vi.fn());
const unlockPortableKeystoreWithBiometricsMock = vi.hoisted(() => vi.fn());

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('@/lib/api', () => ({
  api: {
    setupPortableKeystore: setupPortableKeystoreMock,
    unlockPortableKeystore: unlockPortableKeystoreMock,
    unlockPortableKeystoreWithBiometrics: unlockPortableKeystoreWithBiometricsMock,
  },
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ open, children }: { open: boolean; children: React.ReactNode }) => open ? <div>{children}</div> : null,
  DialogContent: ({ children, className }: { children: React.ReactNode; className?: string }) => <div className={className}>{children}</div>,
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

import { PortableBootstrapShell } from '@/components/bootstrap/PortableBootstrapShell';

const defaultInfo = {
  isPortable: true,
  activation: 'marker' as const,
  hostKind: 'executableDir' as const,
  exeDir: '/portable',
  hostDir: '/portable',
  markerPath: '/portable/portable',
  configPath: '/portable/portable.json',
  dataDir: '/portable/data',
  instanceLockPath: '/portable/data/.portable.lock',
};

const unlockedStatus = {
  isPortable: true,
  activation: 'marker' as const,
  hostKind: 'executableDir' as const,
  status: 'unlocked' as const,
  canLaunchApp: true,
  hasKeystore: true,
  isUnlocked: true,
  keystorePath: '/portable/data/keystore.vault',
  portableRootDir: '/portable',
  markerPath: '/portable/portable',
  configPath: '/portable/portable.json',
  instanceLockPath: '/portable/data/.portable.lock',
  supportsBiometricBinding: false,
  hasBiometricBinding: false,
  canBiometricUnlock: false,
};

describe('PortableBootstrapShell', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('completes setup flow and promotes app launch readiness', async () => {
    const onReady = vi.fn();
    setupPortableKeystoreMock.mockResolvedValue(unlockedStatus);

    render(
      <PortableBootstrapShell
        info={defaultInfo}
        status={{
          isPortable: true,
          activation: 'marker',
          hostKind: 'executableDir',
          status: 'needsSetup',
          canLaunchApp: false,
          hasKeystore: false,
          isUnlocked: false,
          keystorePath: null,
          portableRootDir: '/portable',
          markerPath: '/portable/portable',
          configPath: '/portable/portable.json',
          instanceLockPath: '/portable/data/.portable.lock',
          supportsBiometricBinding: false,
          hasBiometricBinding: false,
          canBiometricUnlock: false,
        }}
        onReady={onReady}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.setup_cta' }));
    fireEvent.change(screen.getByLabelText('portable_bootstrap.password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.change(screen.getByLabelText('portable_bootstrap.confirm_password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.setup_submit' }));

    await waitFor(() => {
      expect(setupPortableKeystoreMock).toHaveBeenCalledWith('secret123');
      expect(onReady).toHaveBeenCalledWith(expect.objectContaining({
        status: 'unlocked',
        canLaunchApp: true,
      }));
    });
  });

  it('blocks setup submission when passwords do not match', async () => {
    render(
      <PortableBootstrapShell
        info={defaultInfo}
        status={{
          isPortable: true,
          activation: 'marker',
          hostKind: 'executableDir',
          status: 'needsSetup',
          canLaunchApp: false,
          hasKeystore: false,
          isUnlocked: false,
          keystorePath: null,
          portableRootDir: '/portable',
          markerPath: '/portable/portable',
          configPath: '/portable/portable.json',
          instanceLockPath: '/portable/data/.portable.lock',
          supportsBiometricBinding: false,
          hasBiometricBinding: false,
          canBiometricUnlock: false,
        }}
        onReady={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.setup_cta' }));
    fireEvent.change(screen.getByLabelText('portable_bootstrap.password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.change(screen.getByLabelText('portable_bootstrap.confirm_password_label'), {
      target: { value: 'different123' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.setup_submit' }));

    expect(setupPortableKeystoreMock).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent('portable_bootstrap.password_mismatch');
  });

  it('completes unlock flow when a keystore already exists', async () => {
    const onReady = vi.fn();
    unlockPortableKeystoreMock.mockResolvedValue(unlockedStatus);

    render(
      <PortableBootstrapShell
        info={defaultInfo}
        status={{
          isPortable: true,
          activation: 'marker',
          hostKind: 'executableDir',
          status: 'locked',
          canLaunchApp: false,
          hasKeystore: true,
          isUnlocked: false,
          keystorePath: '/portable/data/keystore.vault',
          portableRootDir: '/portable',
          markerPath: '/portable/portable',
          configPath: '/portable/portable.json',
          instanceLockPath: '/portable/data/.portable.lock',
          supportsBiometricBinding: false,
          hasBiometricBinding: false,
          canBiometricUnlock: false,
        }}
        onReady={onReady}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.unlock_cta' }));
    fireEvent.change(screen.getByLabelText('portable_bootstrap.password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.unlock_submit' }));

    await waitFor(() => {
      expect(unlockPortableKeystoreMock).toHaveBeenCalledWith('secret123');
      expect(onReady).toHaveBeenCalledWith(expect.objectContaining({
        status: 'unlocked',
        canLaunchApp: true,
      }));
    });
  });

  it('submits setup with Enter from the confirmation field', async () => {
    const onReady = vi.fn();
    setupPortableKeystoreMock.mockResolvedValue(unlockedStatus);

    render(
      <PortableBootstrapShell
        info={defaultInfo}
        status={{
          isPortable: true,
          activation: 'marker',
          hostKind: 'executableDir',
          status: 'needsSetup',
          canLaunchApp: false,
          hasKeystore: false,
          isUnlocked: false,
          keystorePath: null,
          portableRootDir: '/portable',
          markerPath: '/portable/portable',
          configPath: '/portable/portable.json',
          instanceLockPath: '/portable/data/.portable.lock',
          supportsBiometricBinding: false,
          hasBiometricBinding: false,
          canBiometricUnlock: false,
        }}
        onReady={onReady}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.setup_cta' }));
    fireEvent.change(screen.getByLabelText('portable_bootstrap.password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.change(screen.getByLabelText('portable_bootstrap.confirm_password_label'), {
      target: { value: 'secret123' },
    });
    fireEvent.submit(screen.getByLabelText('portable_bootstrap.confirm_password_label').closest('form')!);

    await waitFor(() => {
      expect(setupPortableKeystoreMock).toHaveBeenCalledWith('secret123');
      expect(onReady).toHaveBeenCalled();
    });
  });

  it('shows biometric unlock when available and uses the biometric command', async () => {
    const onReady = vi.fn();
    unlockPortableKeystoreWithBiometricsMock.mockResolvedValue({
      ...unlockedStatus,
      supportsBiometricBinding: true,
      hasBiometricBinding: true,
    });

    render(
      <PortableBootstrapShell
        info={defaultInfo}
        status={{
          isPortable: true,
          activation: 'marker',
          hostKind: 'executableDir',
          status: 'locked',
          canLaunchApp: false,
          hasKeystore: true,
          isUnlocked: false,
          keystorePath: '/portable/data/keystore.vault',
          portableRootDir: '/portable',
          markerPath: '/portable/portable',
          configPath: '/portable/portable.json',
          instanceLockPath: '/portable/data/.portable.lock',
          supportsBiometricBinding: true,
          hasBiometricBinding: true,
          canBiometricUnlock: true,
        }}
        onReady={onReady}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'portable_bootstrap.biometric_unlock_cta' }));

    await waitFor(() => {
      expect(unlockPortableKeystoreWithBiometricsMock).toHaveBeenCalledTimes(1);
      expect(onReady).toHaveBeenCalledWith(expect.objectContaining({
        status: 'unlocked',
      }));
    });
  });
});