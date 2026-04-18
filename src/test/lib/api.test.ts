import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { api } from '@/lib/api';

describe('portable API bindings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it('calls get_portable_status', async () => {
    (window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: () => void };
    }).__TAURI_INTERNALS__ = { invoke: () => undefined };

    vi.mocked(invoke).mockResolvedValueOnce({
      isPortable: false,
      activation: 'disabled',
      hostKind: 'executableDir',
      status: 'disabled',
      canLaunchApp: true,
      hasKeystore: false,
      isUnlocked: false,
      keystorePath: null,
      portableRootDir: '/mock/OxideTerm',
      markerPath: '/mock/OxideTerm/portable',
      configPath: '/mock/OxideTerm/portable.json',
      instanceLockPath: null,
      supportsBiometricBinding: false,
      hasBiometricBinding: false,
      canBiometricUnlock: false,
    });

    await api.getPortableStatus();

    expect(invoke).toHaveBeenCalledWith('get_portable_status');
  });

  it('calls setup_portable_keystore with password', async () => {
    (window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: () => void };
    }).__TAURI_INTERNALS__ = { invoke: () => undefined };

    vi.mocked(invoke).mockResolvedValueOnce({
      isPortable: true,
      activation: 'marker',
      hostKind: 'executableDir',
      status: 'unlocked',
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
    });

    await api.setupPortableKeystore('secret123');

    expect(invoke).toHaveBeenCalledWith('setup_portable_keystore', { password: 'secret123' });
  });

  it('calls unlock_portable_keystore with password', async () => {
    (window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: () => void };
    }).__TAURI_INTERNALS__ = { invoke: () => undefined };

    vi.mocked(invoke).mockResolvedValueOnce({
      isPortable: true,
      activation: 'marker',
      hostKind: 'executableDir',
      status: 'unlocked',
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
    });

    await api.unlockPortableKeystore('secret123');

    expect(invoke).toHaveBeenCalledWith('unlock_portable_keystore', { password: 'secret123' });
  });

  it('calls unlock_portable_keystore_with_biometrics without args', async () => {
    (window as typeof window & {
      __TAURI_INTERNALS__?: { invoke?: () => void };
    }).__TAURI_INTERNALS__ = { invoke: () => undefined };

    vi.mocked(invoke).mockResolvedValueOnce({
      isPortable: true,
      activation: 'marker',
      hostKind: 'executableDir',
      status: 'unlocked',
      canLaunchApp: true,
      hasKeystore: true,
      isUnlocked: true,
      keystorePath: '/portable/data/keystore.vault',
      portableRootDir: '/portable',
      markerPath: '/portable/portable',
      configPath: '/portable/portable.json',
      instanceLockPath: '/portable/data/.portable.lock',
      supportsBiometricBinding: true,
      hasBiometricBinding: true,
      canBiometricUnlock: false,
    });

    await api.unlockPortableKeystoreWithBiometrics();

    expect(invoke).toHaveBeenCalledWith('unlock_portable_keystore_with_biometrics');
  });

  it('returns a safe disabled status when tauri runtime is unavailable', async () => {
    await expect(api.getPortableStatus()).resolves.toEqual({
      isPortable: false,
      activation: 'disabled',
      hostKind: 'executableDir',
      status: 'disabled',
      canLaunchApp: true,
      hasKeystore: false,
      isUnlocked: false,
      keystorePath: null,
      portableRootDir: '/mock/OxideTerm',
      markerPath: '/mock/OxideTerm/portable',
      configPath: '/mock/OxideTerm/portable.json',
      instanceLockPath: null,
      supportsBiometricBinding: false,
      hasBiometricBinding: false,
      canBiometricUnlock: false,
    });

    expect(invoke).not.toHaveBeenCalled();
  });
});