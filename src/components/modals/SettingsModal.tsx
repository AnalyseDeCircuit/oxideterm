import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore, type RendererType, type FontFamily } from '../../store/settingsStore';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Checkbox } from '../ui/checkbox';
import { Separator } from '../ui/separator';
import { 
  Dialog, 
  DialogContent, 
  DialogTitle, 
  DialogDescription
} from '../ui/dialog';
import { 
  Select, 
  SelectContent, 
  SelectItem, 
  SelectTrigger, 
  SelectValue 
} from '../ui/select';
import { Monitor, Key, Terminal as TerminalIcon, Shield, Plus, Trash2, FolderInput, X, HardDrive, Sparkles, ExternalLink } from 'lucide-react';
import { api } from '../../lib/api';
import { SshKeyInfo, SshHostInfo } from '../../types';
import { themes } from '../../lib/themes';

const ThemePreview = ({ themeName }: { themeName: string }) => {
    const theme = themes[themeName] || themes.default;
    
    return (
        <div className="mt-2 p-3 rounded-md border border-theme-border" style={{ backgroundColor: theme.background }}>
            <div className="flex gap-2 mb-2">
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.red }}></div>
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.yellow }}></div>
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.green }}></div>
            </div>
            <div className="font-mono text-xs space-y-1" style={{ color: theme.foreground }}>
                <div>$ echo "Hello World"</div>
                <div style={{ color: theme.blue }}>~ <span style={{ color: theme.magenta }}>git</span> status</div>
                <div className="flex items-center">
                    <span>&gt; </span>
                    <span className="w-2 h-4 ml-1 animate-pulse" style={{ backgroundColor: theme.cursor }}></span>
                </div>
            </div>
        </div>
    );
};

export const SettingsModal = () => {
  const { t } = useTranslation();
  const { modals, toggleModal, createTab } = useAppStore();
  const [activeTab, setActiveTab] = useState('terminal');
  
  // Use unified settings store
  const { settings, updateTerminal, updateBuffer, updateAppearance, updateConnectionDefaults, updateSftp } = useSettingsStore();
  const { terminal, buffer, appearance, connectionDefaults, sftp } = settings;
  

  
  // Data State
  const [keys, setKeys] = useState<SshKeyInfo[]>([]);
  const [groups, setGroups] = useState<string[]>([]);
  const [newGroup, setNewGroup] = useState('');
  const [sshHosts, setSshHosts] = useState<SshHostInfo[]>([]);
  
  useEffect(() => {
      if (modals.settings) {
          if (activeTab === 'ssh') {
              api.checkSshKeys()
                .then(setKeys)
                .catch((e) => {
                  console.error('Failed to load SSH keys:', e);
                  setKeys([]);
                });
          } else if (activeTab === 'connections') {
              api.getGroups()
                .then(setGroups)
                .catch((e) => {
                  console.error('Failed to load groups:', e);
                  setGroups([]);
                });
              api.listSshConfigHosts()
                .then(setSshHosts)
                .catch((e) => {
                  console.error('Failed to load SSH hosts:', e);
                  setSshHosts([]);
                });
          }
      }
  }, [activeTab, modals.settings]);

  const handleCreateGroup = async () => {
      if (!newGroup.trim()) return;
      try {
          await api.createGroup(newGroup.trim());
          setNewGroup('');
          const updatedGroups = await api.getGroups();
          setGroups(updatedGroups);
      } catch (e) {
          console.error('Failed to create group:', e);
          alert(t('modals.settings.errors.create_group_failed', { error: e }));
      }
  };

  const handleDeleteGroup = async (name: string) => {
      try {
          await api.deleteGroup(name);
          const updatedGroups = await api.getGroups();
          setGroups(updatedGroups);
      } catch (e) {
          console.error('Failed to delete group:', e);
          alert(t('modals.settings.errors.delete_group_failed', { error: e }));
      }
  };

  const handleImportHost = async (alias: string) => {
      try {
          const imported = await api.importSshHost(alias);
          alert(t('modals.settings.errors.import_host_success', { name: imported.name }));
          // Remove from list to show it's imported
          setSshHosts(prev => prev.filter(h => h.alias !== alias));
          // Refresh saved connections in sidebar
          const { loadSavedConnections } = useAppStore.getState();
          await loadSavedConnections();
      } catch (e) {
          console.error('Failed to import SSH host:', e);
          alert(t('modals.settings.errors.import_host_failed', { error: e }));
      }
  };

  return (
    <Dialog open={modals.settings} onOpenChange={(open) => toggleModal('settings', open)}>
      <DialogContent className="max-w-4xl h-[600px] flex flex-col p-0 gap-0 overflow-hidden sm:rounded-md" aria-describedby="settings-desc">
        <DialogTitle className="sr-only">{t('modals.settings.title')}</DialogTitle>
        <DialogDescription id="settings-desc" className="sr-only">
            {t('modals.settings.description')}
        </DialogDescription>
        
        <div className="flex h-full">
            {/* Sidebar */}
            <div className="w-48 bg-theme-bg-panel border-r border-theme-border flex flex-col pt-4 pb-4 min-h-0">
                <div className="px-4 mb-4 flex items-center justify-between">
                    <h2 className="text-sm font-semibold text-zinc-100">{t('modals.settings.title')}</h2>
                    <Button 
                        size="icon" 
                        variant="ghost" 
                        className="h-6 w-6" 
                        onClick={() => toggleModal('settings', false)}
                    >
                        <X className="h-4 w-4" />
                    </Button>
                </div>
                <div className="space-y-1 px-2 flex-1 overflow-y-auto min-h-0">
                    <Button 
                        variant={activeTab === 'terminal' ? 'secondary' : 'ghost'} 
                        className="w-full justify-start gap-2 h-8"
                        onClick={() => setActiveTab('terminal')}
                    >
                        <TerminalIcon className="h-4 w-4" /> {t('modals.settings.tabs.terminal')}
                    </Button>
                    <Button 
                        variant={activeTab === 'sftp' ? 'secondary' : 'ghost'} 
                        className="w-full justify-start gap-2 h-8"
                        onClick={() => setActiveTab('sftp')}
                    >
                        <HardDrive className="h-4 w-4" /> {t('modals.settings.tabs.sftp')}
                    </Button>
                    <Button 
                        variant={activeTab === 'appearance' ? 'secondary' : 'ghost'} 
                        className="w-full justify-start gap-2 h-8"
                        onClick={() => setActiveTab('appearance')}
                    >
                        <Monitor className="h-4 w-4" /> {t('modals.settings.tabs.appearance')}
                    </Button>
                    <Button 
                        variant={activeTab === 'connections' ? 'secondary' : 'ghost'} 
                        className="w-full justify-start gap-2 h-8"
                        onClick={() => setActiveTab('connections')}
                    >
                        <Shield className="h-4 w-4" /> {t('modals.settings.tabs.connections')}
                    </Button>
                    <Button 
                        variant={activeTab === 'ssh' ? 'secondary' : 'ghost'} 
                        className="w-full justify-start gap-2 h-8"
                        onClick={() => setActiveTab('ssh')}
                    >
                        <Key className="h-4 w-4" /> {t('modals.settings.tabs.ssh')}
                    </Button>
                    <Button 
                        variant="ghost"
                        className="w-full justify-start gap-2 h-8 text-zinc-400 hover:text-zinc-200"
                        onClick={() => {
                            toggleModal('settings', false);
                            createTab('settings');
                        }}
                    >
                        <Sparkles className="h-4 w-4" /> {t('modals.settings.tabs.ai')}
                        <ExternalLink className="h-3 w-3 ml-auto opacity-50" />
                    </Button>
                </div>
            </div>

            {/* Content */}
            <div className="flex-1 bg-theme-bg overflow-y-auto p-6">
                {activeTab === 'terminal' && (
                    <div className="space-y-6">
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.terminal.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.terminal.description')}</p>
                        </div>
                        <Separator />
                        
                        <div className="grid gap-4">
                            <div className="grid grid-cols-2 gap-4">
                                <div className="grid gap-2">
                                    <Label>{t('modals.settings.terminal.font_family')}</Label>
                                    <Select 
                                        value={terminal.fontFamily}
                                        onValueChange={(v) => updateTerminal('fontFamily', v as FontFamily)}
                                    >
                                        <SelectTrigger>
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="jetbrains">JetBrains Mono NF ✓</SelectItem>
                                            <SelectItem value="meslo">MesloLGM NF ✓</SelectItem>
                                            <SelectItem value="maple">Maple Mono NF CN ✓</SelectItem>
                                            <SelectItem value="cascadia">Cascadia Code</SelectItem>
                                            <SelectItem value="consolas">Consolas</SelectItem>
                                            <SelectItem value="menlo">Menlo</SelectItem>
                                            <SelectItem value="custom">{t('modals.settings.terminal.custom_font')}</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                                <div className="grid gap-2">
                                    <Label>{t('modals.settings.terminal.font_size')}</Label>
                                    <Select 
                                        value={terminal.fontSize.toString()}
                                        onValueChange={(v) => updateTerminal('fontSize', parseInt(v))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {[10, 11, 12, 13, 14, 15, 16, 18, 20, 24].map(size => (
                                                <SelectItem key={size} value={size.toString()}>{size}px</SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            {/* 自定义轨道: Custom Font Input */}
                            {terminal.fontFamily === 'custom' && (
                                <div className="grid gap-2">
                                    <Label>{t('modals.settings.terminal.custom_font_stack')}</Label>
                                    <Input
                                        type="text"
                                        value={terminal.customFontFamily}
                                        onChange={(e) => updateTerminal('customFontFamily', e.target.value)}
                                        placeholder="'Sarasa Fixed SC', 'Fira Code', monospace"
                                        className="font-mono text-sm"
                                    />
                                    <p className="text-xs text-zinc-500">{t('modals.settings.terminal.custom_font_stack_hint')}</p>
                                </div>
                            )}

                            {/* 字体预览 */}
                            <div className="rounded-md border border-zinc-800 bg-zinc-950 p-3">
                                <p className="text-xs text-zinc-500 mb-2">{t('modals.settings.terminal.font_preview')}</p>
                                <div 
                                    className="text-zinc-100 leading-relaxed"
                                    style={{ 
                                        fontFamily: terminal.fontFamily === 'custom' && terminal.customFontFamily 
                                            ? (terminal.customFontFamily.toLowerCase().includes('monospace') 
                                                ? terminal.customFontFamily.replace(/,?\s*monospace\s*$/, ', "Maple Mono NF CN", monospace')
                                                : `${terminal.customFontFamily}, "Maple Mono NF CN", monospace`)
                                            : terminal.fontFamily === 'jetbrains' ? '"JetBrainsMono Nerd Font", "JetBrains Mono NF", "Maple Mono NF CN", monospace'
                                            : terminal.fontFamily === 'meslo' ? '"MesloLGM Nerd Font", "MesloLGM NF", "Maple Mono NF CN", monospace'
                                            : terminal.fontFamily === 'maple' ? '"Maple Mono NF CN", "Maple Mono NF", monospace'
                                            : terminal.fontFamily === 'cascadia' ? '"Cascadia Code NF", "Cascadia Code", "Maple Mono NF CN", monospace'
                                            : terminal.fontFamily === 'consolas' ? 'Consolas, "Maple Mono NF CN", monospace'
                                            : terminal.fontFamily === 'menlo' ? 'Menlo, Monaco, "Maple Mono NF CN", monospace'
                                            : '"Maple Mono NF CN", monospace',
                                        fontSize: `${terminal.fontSize}px`,
                                        lineHeight: terminal.lineHeight,
                                    }}
                                >
                                    <div>ABCabc 0123456789</div>
                                    <div className="text-zinc-400">{'-> => == {}'}</div>
                                    <div className="text-emerald-400">天地 Fox</div>
                                    <div className="text-amber-400" style={{ letterSpacing: '0.1em' }}>    </div>
                                </div>
                            </div>

                            <div className="grid grid-cols-2 gap-4">
                                <div className="grid gap-2">
                                    <Label>{t('modals.settings.terminal.line_height')}</Label>
                                    <Select 
                                        value={terminal.lineHeight.toString()}
                                        onValueChange={(v) => updateTerminal('lineHeight', parseFloat(v))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {['1.0', '1.1', '1.2', '1.3', '1.4', '1.5'].map(h => (
                                                <SelectItem key={h} value={h}>{h}</SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </div>
                                <div className="grid gap-2">
                                    <Label>{t('modals.settings.terminal.scrollback')}</Label>
                                    <Select 
                                        value={terminal.scrollback.toString()}
                                        onValueChange={(v) => updateTerminal('scrollback', parseInt(v))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                            {['1000', '5000', '10000'].map(l => (
                                                <SelectItem key={l} value={l}>{l}</SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            <div className="grid gap-2">
                                <Label>{t('modals.settings.terminal.renderer')}</Label>
                                <Select 
                                    value={terminal.renderer}
                                    onValueChange={(v) => updateTerminal('renderer', v as RendererType)}
                                >
                                    <SelectTrigger className="w-[240px]">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="auto">{t('modals.settings.terminal.renderer_auto')}</SelectItem>
                                        <SelectItem value="webgl">{t('modals.settings.terminal.renderer_webgl')}</SelectItem>
                                        <SelectItem value="canvas">{t('modals.settings.terminal.renderer_canvas')}</SelectItem>
                                    </SelectContent>
                                </Select>
                                <p className="text-xs text-zinc-500">
                                    {t('modals.settings.terminal.renderer_hint')}
                                </p>
                            </div>
                            
                            <div className="grid gap-2 pt-2">
                                <Label>{t('modals.settings.terminal.cursor_style')}</Label>
                                <div className="flex gap-4">
                                    <div className="flex items-center space-x-2">
                                        <Checkbox 
                                            id="block" 
                                            checked={terminal.cursorStyle === 'block'}
                                            onCheckedChange={() => updateTerminal('cursorStyle', 'block')}
                                        />
                                        <Label htmlFor="block">{t('modals.settings.terminal.cursor_block')}</Label>
                                    </div>
                                    <div className="flex items-center space-x-2">
                                        <Checkbox 
                                            id="underline" 
                                            checked={terminal.cursorStyle === 'underline'}
                                            onCheckedChange={() => updateTerminal('cursorStyle', 'underline')}
                                        />
                                        <Label htmlFor="underline">{t('modals.settings.terminal.cursor_underline')}</Label>
                                    </div>
                                    <div className="flex items-center space-x-2">
                                        <Checkbox 
                                            id="bar" 
                                            checked={terminal.cursorStyle === 'bar'}
                                            onCheckedChange={() => updateTerminal('cursorStyle', 'bar')}
                                        />
                                        <Label htmlFor="bar">{t('modals.settings.terminal.cursor_bar')}</Label>
                                    </div>
                                </div>
                            </div>
                            
                            <div className="flex items-center space-x-2">
                                <Checkbox 
                                    id="blink" 
                                    checked={terminal.cursorBlink}
                                    onCheckedChange={(c) => updateTerminal('cursorBlink', !!c)}
                                />
                                <Label htmlFor="blink">{t('modals.settings.terminal.cursor_blink')}</Label>
                            </div>
                        </div>

                        {/* Buffer Settings */}
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.buffer.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.buffer.description')}</p>
                        </div>
                        <Separator />
                        
                        <div className="grid gap-4">
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.buffer.max_lines')}</Label>
                                <Select 
                                    value={buffer.maxLines.toString()}
                                    onValueChange={(v) => updateBuffer('maxLines', parseInt(v))}
                                >
                                    <SelectTrigger>
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="10000">10,000 lines (~1 MB)</SelectItem>
                                        <SelectItem value="50000">50,000 lines (~5 MB)</SelectItem>
                                        <SelectItem value="100000">100,000 lines (~10 MB)</SelectItem>
                                        <SelectItem value="500000">500,000 lines (~50 MB)</SelectItem>
                                        <SelectItem value="1000000">1,000,000 lines (~100 MB)</SelectItem>
                                    </SelectContent>
                                </Select>
                                <p className="text-xs text-zinc-500">
                                    {t('modals.settings.buffer.lines_hint')}
                                </p>
                                <p className="text-xs text-yellow-500">
                                    {t('modals.settings.buffer.new_sessions_only')}
                                </p>
                            </div>
                            
                            <div className="flex items-center space-x-2">
                                <Checkbox 
                                    id="buffer-save" 
                                    checked={buffer.saveOnDisconnect}
                                    onCheckedChange={(c) => updateBuffer('saveOnDisconnect', !!c)}
                                />
                                <Label htmlFor="buffer-save" className="cursor-pointer">
                                    {t('modals.settings.buffer.save_on_disconnect')}
                                </Label>
                            </div>
                            <p className="text-xs text-zinc-500 -mt-2 ml-6">
                                {t('modals.settings.buffer.save_hint')}
                            </p>
                        </div>
                    </div>
                )}

                {activeTab === 'appearance' && (
                    <div className="space-y-6">
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.appearance.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.appearance.description')}</p>
                        </div>
                        <Separator />
                        <div className="grid gap-4">
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.appearance.theme')}</Label>
                                <Select 
                                    value={terminal.theme} 
                                    onValueChange={(v) => updateTerminal('theme', v)}
                                >
                                    <SelectTrigger className="w-[240px]">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="default">Neutral</SelectItem>
                                        <SelectItem value="oxide">Oxide</SelectItem>
                                        <SelectItem value="dracula">Dracula</SelectItem>
                                        <SelectItem value="nord">Nord</SelectItem>
                                        <SelectItem value="solarized-dark">Solarized Dark</SelectItem>
                                        <SelectItem value="monokai">Monokai</SelectItem>
                                        <SelectItem value="github-dark">GitHub Dark</SelectItem>
                                    </SelectContent>
                                </Select>
                                <ThemePreview themeName={terminal.theme} />
                            </div>

                             <div className="flex items-center space-x-2">
                                <Checkbox 
                                    id="sidebar-col" 
                                    checked={appearance.sidebarCollapsedDefault}
                                    onCheckedChange={(c) => updateAppearance('sidebarCollapsedDefault', !!c)}
                                />
                                <Label htmlFor="sidebar-col">{t('modals.settings.appearance.sidebar_collapse')}</Label>
                            </div>
                        </div>
                    </div>
                )}

                 {activeTab === 'connections' && (
                    <div className="space-y-6">
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.connections.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.connections.description')}</p>
                        </div>
                        <Separator />
                        <div className="grid grid-cols-2 gap-4">
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.connections.default_username')}</Label>
                                <Input 
                                    value={connectionDefaults.username}
                                    onChange={(e) => updateConnectionDefaults('username', e.target.value)}
                                />
                            </div>
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.connections.default_port')}</Label>
                                <Input 
                                    value={connectionDefaults.port}
                                    onChange={(e) => updateConnectionDefaults('port', parseInt(e.target.value) || 22)}
                                />
                            </div>
                        </div>

                        <div className="pt-4">
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.connections.groups.title')}</h3>
                            <p className="text-sm text-zinc-500 mb-2">{t('modals.settings.connections.groups.description')}</p>
                            <Separator className="mb-2" />
                            
                            <div className="flex gap-2 mb-2">
                                <Input 
                                    placeholder={t('modals.settings.connections.groups.new_placeholder')}
                                    value={newGroup}
                                    onChange={(e) => setNewGroup(e.target.value)}
                                    className="h-8"
                                />
                                <Button size="sm" onClick={handleCreateGroup} disabled={!newGroup}>
                                    <Plus className="h-3 w-3 mr-1" /> {t('modals.settings.connections.groups.add')}
                                </Button>
                            </div>
                            
                            <div className="space-y-1">
                                {groups.map(group => (
                                    <div key={group} className="flex items-center justify-between p-2 bg-theme-bg-panel rounded-sm border border-theme-border">
                                        <span className="text-sm">{group}</span>
                                        <Button size="icon" variant="ghost" className="h-6 w-6 text-zinc-500 hover:text-red-400" onClick={() => handleDeleteGroup(group)}>
                                            <Trash2 className="h-3 w-3" />
                                        </Button>
                                    </div>
                                ))}
                            </div>
                        </div>

                        <div className="pt-4">
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.connections.ssh_config.title')}</h3>
                            <p className="text-sm text-zinc-500 mb-2">{t('modals.settings.connections.ssh_config.description')}</p>
                            <Separator className="mb-2" />
                            
                            <div className="h-32 overflow-y-auto border border-theme-border rounded-sm bg-theme-bg-panel p-1">
                                {sshHosts.map(host => (
                                    <div key={host.alias} className="flex items-center justify-between p-2 hover:bg-zinc-800 rounded-sm">
                                        <div className="flex flex-col">
                                            <span className="text-sm font-medium">{host.alias}</span>
                                            <span className="text-xs text-zinc-500">{host.user}@{host.hostname}:{host.port}</span>
                                        </div>
                                        <Button size="sm" variant="secondary" className="h-7" onClick={() => handleImportHost(host.alias)}>
                                            <FolderInput className="h-3 w-3 mr-1" /> {t('modals.settings.connections.ssh_config.import')}
                                        </Button>
                                    </div>
                                ))}
                                {sshHosts.length === 0 && (
                                    <div className="text-center py-8 text-zinc-500 text-sm">
                                        {t('modals.settings.connections.ssh_config.no_hosts')}
                                    </div>
                                )}
                            </div>
                        </div>
                    </div>
                )}

                {activeTab === 'ssh' && (
                    <div className="space-y-6">
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.ssh_keys.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.ssh_keys.description')}</p>
                        </div>
                        <Separator />
                        
                        <div className="space-y-2">
                            {keys.map(key => (
                                <div key={key.name} className="flex items-center justify-between p-3 bg-theme-bg-panel border border-theme-border rounded-sm">
                                    <div className="flex items-center gap-3">
                                        <Key className="h-5 w-5 text-theme-accent" />
                                        <div className="flex flex-col">
                                            <span className="text-sm font-medium text-zinc-200">{key.name}</span>
                                            <span className="text-xs text-zinc-500">{key.key_type} · {key.path}</span>
                                        </div>
                                    </div>
                                    {key.has_passphrase && (
                                        <span className="text-xs bg-yellow-900/30 text-yellow-500 px-2 py-0.5 rounded">{t('modals.settings.ssh_keys.encrypted')}</span>
                                    )}
                                </div>
                            ))}
                            {keys.length === 0 && (
                                <div className="text-center py-8 text-zinc-500">
                                    {t('modals.settings.ssh_keys.no_keys')}
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {activeTab === 'sftp' && (
                    <div className="space-y-6">
                        <div>
                            <h3 className="text-lg font-medium text-zinc-100 mb-1">{t('modals.settings.sftp.title')}</h3>
                            <p className="text-sm text-zinc-500">{t('modals.settings.sftp.description')}</p>
                        </div>
                        <Separator />
                        
                        <div className="grid gap-6">
                            {/* Concurrent Transfers */}
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.sftp.concurrent.title')}</Label>
                                <Select 
                                    value={(sftp?.maxConcurrentTransfers ?? 3).toString()}
                                    onValueChange={(v) => updateSftp('maxConcurrentTransfers', parseInt(v))}
                                >
                                    <SelectTrigger className="w-48">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {[1, 2, 3, 4, 5, 6, 8, 10].map(num => (
                                            <SelectItem key={num} value={num.toString()}>
                                                {t('modals.settings.sftp.concurrent.transfer_count', { count: num })}
                                            </SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                                <p className="text-xs text-zinc-500">
                                    {t('modals.settings.sftp.concurrent.hint')}
                                </p>
                            </div>

                            <Separator />

                            {/* Bandwidth Limit */}
                            <div className="grid gap-4">
                                <div className="flex items-center gap-2">
                                    <Checkbox 
                                        id="speed-limit-enabled"
                                        checked={sftp?.speedLimitEnabled ?? false}
                                        onCheckedChange={(checked) => updateSftp('speedLimitEnabled', !!checked)}
                                    />
                                    <Label htmlFor="speed-limit-enabled" className="cursor-pointer">
                                        {t('modals.settings.sftp.bandwidth.enable')}
                                    </Label>
                                </div>
                                
                                {sftp?.speedLimitEnabled && (
                                    <div className="grid gap-2 pl-6">
                                        <Label>{t('modals.settings.sftp.bandwidth.limit')}</Label>
                                        <Input 
                                            type="number"
                                            className="w-48"
                                            value={sftp?.speedLimitKBps ?? 0}
                                            onChange={(e) => {
                                                const value = parseInt(e.target.value) || 0;
                                                updateSftp('speedLimitKBps', Math.max(0, value));
                                            }}
                                            min={0}
                                            step={100}
                                            placeholder="0 = unlimited"
                                        />
                                        <p className="text-xs text-zinc-500">
                                            {t('modals.settings.sftp.bandwidth.hint')}
                                        </p>
                                    </div>
                                )}
                            </div>

                            <Separator />

                            {/* Conflict Resolution */}
                            <div className="grid gap-2">
                                <Label>{t('modals.settings.sftp.conflict.title')}</Label>
                                <Select 
                                    value={sftp?.conflictAction ?? 'ask'}
                                    onValueChange={(v) => updateSftp('conflictAction', v as 'ask' | 'overwrite' | 'skip' | 'rename')}
                                >
                                    <SelectTrigger className="w-48">
                                        <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="ask">{t('modals.settings.sftp.conflict.ask')}</SelectItem>
                                        <SelectItem value="overwrite">{t('modals.settings.sftp.conflict.overwrite')}</SelectItem>
                                        <SelectItem value="skip">{t('modals.settings.sftp.conflict.skip')}</SelectItem>
                                        <SelectItem value="rename">{t('modals.settings.sftp.conflict.rename')}</SelectItem>
                                    </SelectContent>
                                </Select>
                                <p className="text-xs text-zinc-500">
                                    {t('modals.settings.sftp.conflict.hint')}
                                </p>
                            </div>
                        </div>
                    </div>
                )}
            </div>
        </div>
      </DialogContent>
    </Dialog>
  );
};
