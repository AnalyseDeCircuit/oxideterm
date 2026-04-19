import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createMutableSelectorStore } from '@/test/helpers/mockStore';

const apiMocks = vi.hoisted(() => ({
  getDataDirectory: vi.fn(),
  cliGetStatus: vi.fn(),
  getPortableStatus: vi.fn(),
  getPortableMigrationSummary: vi.fn(),
  changePortableKeystorePassword: vi.fn(),
  enablePortableBiometricUnlock: vi.fn(),
  disablePortableBiometricUnlock: vi.fn(),
}));

const toastMocks = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
}));

const confirmMock = vi.hoisted(() => vi.fn().mockResolvedValue(true));

const settingsStoreState = vi.hoisted(() => ({
  settings: {
    general: {
      language: 'en',
      updateChannel: 'stable',
    },
    terminal: {
      fontFamily: 'jetbrains-mono',
      customFontFamily: '',
    },
    appearance: {},
    connectionDefaults: {},
    ai: {
      enabled: false,
      enabledConfirmed: false,
      providers: [],
      activeProviderId: null,
      modelContextWindows: {},
      userContextWindows: {},
      contextMaxChars: 4000,
      contextVisibleLines: 100,
      contextSources: { ide: true, sftp: true },
    },
    sftp: {},
    ide: {},
    reconnect: {},
    connectionPool: {
      idleTimeoutSecs: 1800,
    },
  },
  updateTerminal: vi.fn(),
  updateAppearance: vi.fn(),
  updateConnectionDefaults: vi.fn(),
  updateAi: vi.fn(),
  updateSftp: vi.fn(),
  updateIde: vi.fn(),
  updateReconnect: vi.fn(),
  updateConnectionPool: vi.fn(),
  setLanguage: vi.fn(),
  addProvider: vi.fn(),
  removeProvider: vi.fn(),
  updateProvider: vi.fn(),
  setActiveProvider: vi.fn(),
  refreshProviderModels: vi.fn(),
  setUserContextWindow: vi.fn(),
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => (typeof fallback === 'string' ? fallback : key),
  }),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: createMutableSelectorStore(settingsStoreState),
}));

vi.mock('@/hooks/useTabBackground', () => ({
  useTabBgActive: () => false,
}));

vi.mock('@/hooks/useToast', () => ({
  useToast: () => toastMocks,
}));

vi.mock('@/hooks/useConfirm', () => ({
  useConfirm: () => ({
    confirm: confirmMock,
    ConfirmDialog: null,
  }),
}));

vi.mock('@/components/settings/DocumentManager', () => ({
  DocumentManager: () => <div>DocumentManager</div>,
}));

vi.mock('@/components/settings/KeybindingEditorSection', () => ({
  KeybindingEditorSection: () => <div>KeybindingEditorSection</div>,
}));

vi.mock('@/components/settings/LocalTerminalSettings', () => ({
  LocalTerminalSettings: () => <div>LocalTerminalSettings</div>,
}));

vi.mock('@/components/settings/HelpAboutSection', () => ({
  HelpAboutSection: () => <div>HelpAboutSection</div>,
}));

vi.mock('@/components/settings/tabs/TerminalTab', () => ({
  TerminalTab: () => <div>TerminalTab</div>,
}));

vi.mock('@/components/settings/tabs/AppearanceTab', () => ({
  AppearanceTab: () => <div>AppearanceTab</div>,
}));

vi.mock('@/components/settings/tabs/ConnectionsTab', () => ({
  ConnectionsTab: () => <div>ConnectionsTab</div>,
}));

vi.mock('@/components/settings/tabs/SshTab', () => ({
  SshTab: () => <div>SshTab</div>,
}));

vi.mock('@/components/settings/tabs/ReconnectTab', () => ({
  ReconnectTab: () => <div>ReconnectTab</div>,
}));

vi.mock('@/components/settings/tabs/SftpTab', () => ({
  SftpTab: () => <div>SftpTab</div>,
}));

vi.mock('@/components/settings/tabs/IdeTab', () => ({
  IdeTab: () => <div>IdeTab</div>,
}));

vi.mock('@/components/settings/tabs/AiTab', () => ({
  AiTab: () => <div>AiTab</div>,
}));

vi.mock('@/components/modals/OxideExportModal', () => ({
  OxideExportModal: () => null,
}));

vi.mock('@/components/modals/OxideImportModal', () => ({
  OxideImportModal: () => null,
}));

vi.mock('@/components/settings/portable/PortableBiometricBindingDialog', () => ({
  PortableBiometricBindingDialog: () => null,
}));

vi.mock('@/components/settings/portable/PortablePasswordChangeDialog', () => ({
  PortablePasswordChangeDialog: () => null,
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: ({ open, children }: { open: boolean; children: React.ReactNode }) => (open ? <div>{children}</div> : null),
  DialogContent: ({ children, className }: { children: React.ReactNode; className?: string }) => <div className={className}>{children}</div>,
  DialogTitle: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogDescription: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogHeader: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DialogFooter: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, onClick, disabled, type = 'button', ...props }: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button type={type} onClick={onClick} disabled={disabled} {...props}>{children}</button>
  ),
}));

vi.mock('@/components/ui/label', () => ({
  Label: ({ children, className }: React.LabelHTMLAttributes<HTMLLabelElement>) => <label className={className}>{children}</label>,
}));

vi.mock('@/components/ui/input', () => ({
  Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => <input {...props} />,
}));

vi.mock('@/components/ui/separator', () => ({
  Separator: () => <hr />,
}));

vi.mock('@/components/ui/select', () => ({
  Select: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectTrigger: ({ children, className }: { children: React.ReactNode; className?: string }) => <button className={className} type="button">{children}</button>,
  SelectValue: () => <span>SelectValue</span>,
  SelectContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  SelectItem: ({ children, value }: { children: React.ReactNode; value: string }) => <div data-value={value}>{children}</div>,
}));

import { SettingsView } from '@/components/settings/SettingsView';

describe('SettingsView portable loading', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiMocks.getDataDirectory.mockResolvedValue({
      path: '/tmp/oxide',
      is_custom: false,
      default_path: '/tmp/oxide',
      is_portable: false,
      can_change: true,
    });
    apiMocks.cliGetStatus.mockResolvedValue({
      bundled: true,
      installed: false,
      install_path: null,
      bundle_path: '/Applications/OxideTerm.app',
      app_version: '1.2.5',
      matches_bundled: null,
      needs_reinstall: false,
    });
    apiMocks.getPortableStatus.mockResolvedValue({
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
    apiMocks.getPortableMigrationSummary.mockResolvedValue({
      isPortable: true,
      currentDataDir: '/Users/test/.oxideterm',
      portableDataDir: '/portable/data',
      exportablePortableSecretCount: 2,
    });
    apiMocks.changePortableKeystorePassword.mockResolvedValue(undefined);
    apiMocks.enablePortableBiometricUnlock.mockResolvedValue(undefined);
    apiMocks.disablePortableBiometricUnlock.mockResolvedValue(undefined);
  });

  it('does not issue portable requests while General tab is the default view', async () => {
    render(<SettingsView />);

    await waitFor(() => {
      expect(apiMocks.getDataDirectory).toHaveBeenCalledTimes(1);
      expect(apiMocks.cliGetStatus).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByText('settings_view.general.portable_migration_export')).not.toBeInTheDocument();
    expect(apiMocks.getPortableStatus).not.toHaveBeenCalled();
    expect(apiMocks.getPortableMigrationSummary).not.toHaveBeenCalled();
  });

  it('lazy-loads portable data only after switching to the Portable tab', async () => {
    render(<SettingsView />);

    await waitFor(() => {
      expect(apiMocks.getDataDirectory).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByText('settings_view.general.portable_migration_export')).not.toBeInTheDocument();
    expect(apiMocks.getPortableStatus).not.toHaveBeenCalled();
    expect(apiMocks.getPortableMigrationSummary).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'settings_view.general.portable_runtime' }));

    await waitFor(() => {
      expect(apiMocks.getPortableStatus).toHaveBeenCalledTimes(1);
      expect(apiMocks.getPortableMigrationSummary).toHaveBeenCalledTimes(1);
      expect(screen.getByText('settings_view.general.portable_migration_export')).toBeInTheDocument();
    });
  });

  it('shows the disabled portable runtime copy when the profile is not portable', async () => {
    apiMocks.getPortableStatus.mockResolvedValueOnce({
      isPortable: false,
      activation: 'disabled',
      hostKind: 'macAppBundle',
      status: 'disabled',
      canLaunchApp: true,
      hasKeystore: false,
      isUnlocked: false,
      keystorePath: null,
      portableRootDir: '/Applications/OxideTerm.app',
      markerPath: '/Applications/portable',
      configPath: '/Applications/portable.json',
      instanceLockPath: null,
      supportsBiometricBinding: true,
      hasBiometricBinding: false,
      canBiometricUnlock: false,
    });
    apiMocks.getPortableMigrationSummary.mockResolvedValueOnce({
      isPortable: false,
      currentDataDir: '/Users/test/.oxideterm',
      portableDataDir: '/Applications/data',
      exportablePortableSecretCount: 1,
    });

    render(<SettingsView />);

    fireEvent.click(screen.getByRole('button', { name: 'settings_view.general.portable_runtime' }));

    await waitFor(() => {
      expect(apiMocks.getPortableStatus).toHaveBeenCalledTimes(1);
      expect(screen.getAllByText('settings_view.general.portable_runtime_disabled_hint').length).toBeGreaterThan(0);
    });

    expect(screen.queryByText('settings_view.portable_description')).not.toBeInTheDocument();
  });
});