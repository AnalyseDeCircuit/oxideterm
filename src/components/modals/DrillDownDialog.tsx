import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../ui/dialog';
import { Label } from '../ui/label';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Loader2, ArrowDownRight } from 'lucide-react';
import { api } from '../../lib/api';
import { useSessionTreeStore } from '../../store/sessionTreeStore';

interface DrillDownDialogProps {
  /** 父节点 ID */
  parentNodeId: string;
  /** 父节点主机名（用于显示） */
  parentHost: string;
  /** 对话框是否打开 */
  open: boolean;
  /** 关闭对话框回调 */
  onOpenChange: (open: boolean) => void;
  /** 成功后回调 */
  onSuccess?: (nodeId: string, sshConnectionId: string) => void;
}

export const DrillDownDialog: React.FC<DrillDownDialogProps> = ({
  parentNodeId,
  parentHost,
  open,
  onOpenChange,
  onSuccess,
}) => {
  const { t } = useTranslation();
  // 表单状态
  const [host, setHost] = useState('');
  const [port, setPort] = useState('22');
  const [username, setUsername] = useState('');
  const [authType, setAuthType] = useState<'password' | 'key' | 'agent'>('agent');
  const [password, setPassword] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [passphrase, setPassphrase] = useState('');
  
  // 加载状态
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const { fetchTree } = useSessionTreeStore();

  const handleAuthTypeChange = (value: string) => {
    if (value === 'password' || value === 'key' || value === 'agent') {
      setAuthType(value);
    }
  };

  const handleBrowseKey = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: t('modals.drill_down.auth_key'),
        defaultPath: '~/.ssh'
      });
      if (selected && typeof selected === 'string') {
        setKeyPath(selected);
      }
    } catch (e) {
      console.error('Failed to open file dialog:', e);
    }
  };

  const resetForm = () => {
    setHost('');
    setPort('22');
    setUsername('');
    setAuthType('agent');
    setPassword('');
    setKeyPath('');
    setPassphrase('');
    setError(null);
    setIsConnecting(false);
  };

  const handleClose = () => {
    resetForm();
    onOpenChange(false);
  };

  const handleDrillDown = async () => {
    if (!host || !username) return;

    setIsConnecting(true);
    setError(null);

    try {
      // 1. 调用 tree_drill_down 在树中添加子节点
      const nodeId = await api.treeDrillDown({
        parentNodeId,
        host,
        port: parseInt(port) || 22,
        username,
        authType,
        password: authType === 'password' ? password : undefined,
        keyPath: authType === 'key' ? keyPath : undefined,
        passphrase: authType === 'key' && passphrase ? passphrase : undefined,
      });

      // 2. 调用 connect_tree_node 建立实际连接
      const result = await api.connectTreeNode({
        nodeId,
        cols: 80,
        rows: 24,
      });

      // 3. 刷新树
      await fetchTree();

      // 4. 调用成功回调
      onSuccess?.(result.nodeId, result.sshConnectionId);

      // 5. 关闭对话框
      handleClose();
    } catch (err) {
      console.error('Drill down failed:', err);
      setError(err instanceof Error ? err.message : String(err));
      // 刷新树以显示失败状态
      await fetchTree();
    } finally {
      setIsConnecting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ArrowDownRight className="w-5 h-5 text-blue-400" />
            {t('modals.drill_down.title')}
          </DialogTitle>
          <p className="text-sm text-zinc-500 mt-1">
            {t('modals.drill_down.description', { host: parentHost }).split('<host>').map((part, i) => 
              i === 0 ? <span key={i}>{part}</span> : <span key={i}><span className="text-white font-mono">{part.split('</host>')[0]}</span>{part.split('</host>')[1]}</span>
            )}
          </p>
        </DialogHeader>

        <div className="space-y-4 p-4">
          {/* Error message */}
          {error && (
            <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-md text-sm text-red-400">
              {error}
            </div>
          )}

          {/* Host & Port */}
          <div className="grid grid-cols-4 gap-4">
            <div className="col-span-3 space-y-2">
              <Label htmlFor="drill-host">{t('modals.drill_down.target_host')} *</Label>
              <Input
                id="drill-host"
                placeholder={t('modals.drill_down.target_host_placeholder')}
                value={host}
                onChange={(e) => setHost(e.target.value)}
                disabled={isConnecting}
              />
            </div>
            <div className="col-span-1 space-y-2">
              <Label htmlFor="drill-port">{t('modals.drill_down.port')}</Label>
              <Input
                id="drill-port"
                type="number"
                value={port}
                onChange={(e) => setPort(e.target.value)}
                disabled={isConnecting}
              />
            </div>
          </div>

          {/* Username */}
          <div className="space-y-2">
            <Label htmlFor="drill-username">{t('modals.drill_down.username')} *</Label>
            <Input
              id="drill-username"
              placeholder={t('modals.drill_down.username_placeholder')}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              disabled={isConnecting}
            />
          </div>

          {/* Authentication */}
          <div className="space-y-2">
            <Label>{t('modals.drill_down.auth_method')}</Label>
            <Tabs
              value={authType}
              onValueChange={handleAuthTypeChange}
              className="w-full"
            >
              <TabsList className="grid w-full grid-cols-3">
                <TabsTrigger value="agent" disabled={isConnecting}>{t('modals.drill_down.auth_agent')}</TabsTrigger>
                <TabsTrigger value="key" disabled={isConnecting}>{t('modals.drill_down.auth_key')}</TabsTrigger>
                <TabsTrigger value="password" disabled={isConnecting}>{t('modals.drill_down.auth_password')}</TabsTrigger>
              </TabsList>

                <TabsContent value="agent">
                <div className="text-sm text-zinc-400 pt-2 space-y-2">
                  <p>{t('modals.drill_down.agent_desc')}</p>
                  <p className="text-xs text-zinc-500">
                    {t('modals.drill_down.agent_hint')}
                  </p>
                </div>
                </TabsContent>

              <TabsContent value="key">
                <div className="space-y-2 pt-2">
                  <Label htmlFor="drill-keypath">{t('modals.drill_down.key_path')}</Label>
                  <div className="flex gap-2">
                    <Input
                      id="drill-keypath"
                      value={keyPath}
                      onChange={(e) => setKeyPath(e.target.value)}
                      placeholder={t('modals.drill_down.key_path_placeholder')}
                      disabled={isConnecting}
                    />
                    <Button 
                      variant="outline" 
                      onClick={handleBrowseKey} 
                      type="button"
                      disabled={isConnecting}
                    >
                      {t('modals.drill_down.browse')}
                    </Button>
                  </div>
                  <div className="space-y-1 pt-1">
                    <Label htmlFor="drill-passphrase" className="text-sm font-normal">{t('modals.drill_down.passphrase')}</Label>
                    <Input
                      id="drill-passphrase"
                      type="password"
                      value={passphrase}
                      onChange={(e) => setPassphrase(e.target.value)}
                      disabled={isConnecting}
                    />
                  </div>
                </div>
              </TabsContent>

              <TabsContent value="password">
                <div className="space-y-2 pt-2">
                  <Label htmlFor="drill-password">{t('modals.drill_down.password')}</Label>
                  <Input
                    id="drill-password"
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    disabled={isConnecting}
                  />
                </div>
              </TabsContent>
            </Tabs>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={handleClose} disabled={isConnecting}>
            {t('modals.drill_down.cancel')}
          </Button>
          <Button 
            onClick={handleDrillDown} 
            disabled={!host || !username || isConnecting}
          >
            {isConnecting ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                {t('modals.drill_down.connecting')}
              </>
            ) : (
              <>
                <ArrowDownRight className="w-4 h-4 mr-2" />
                {t('modals.drill_down.connect')}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
