import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Dialog, 
  DialogContent, 
  DialogDescription, 
  DialogFooter, 
  DialogHeader, 
  DialogTitle 
} from '../ui/dialog';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { RadioGroup, RadioGroupItem } from '../ui/radio-group';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { ConnectionInfo } from '../../types';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { api } from '../../lib/api';

interface EditConnectionModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  connection: ConnectionInfo | null;
  onConnect?: () => void;
}

export const EditConnectionModal: React.FC<EditConnectionModalProps> = ({
  open,
  onOpenChange,
  connection,
  onConnect
}) => {
  const { t } = useTranslation();
  const { groups, loadGroups } = useAppStore();
  const { connectNodeWithAncestors } = useSessionTreeStore();
  const [password, setPassword] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [authType, setAuthType] = useState<'password' | 'key' | 'agent'>('password');
  const [group, setGroup] = useState('');
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (open && connection) {
      // Map auth_type to modal's authType
      const modalAuthType = connection.auth_type === 'password' ? 'password' : 
                           connection.auth_type === 'agent' ? 'agent' : 'key';
      setAuthType(modalAuthType);
      setKeyPath(connection.key_path || '');
      setGroup(connection.group || '');
      setPassword('');
      setPassphrase('');
      setError('');
      loadGroups();
    }
  }, [open, connection, loadGroups]);

  const handleConnect = async () => {
    if (!connection) return;

    setIsConnecting(true);
    setError('');

    try {
      // Build the preset chain request (direct connection, no hops)
      const target = {
        host: connection.host,
        port: connection.port,
        username: connection.username,
        authType: authType,
        password: authType === 'password' ? password : undefined,
        keyPath: authType === 'key' ? keyPath : undefined,
        passphrase: authType === 'key' && passphrase ? passphrase : undefined,
      };

      // Expand the preset into a session tree node
      const { expandManualPreset } = useSessionTreeStore.getState();
      const result = await expandManualPreset({
        savedConnectionId: connection.id,
        hops: [],
        target,
      });

      // Connect via the session tree
      await connectNodeWithAncestors(result.targetNodeId);

      // Mark as used
      await api.markConnectionUsed(connection.id);
      
      onOpenChange(false);
      if (onConnect) onConnect();
    } catch (e: unknown) {
      console.error('Failed to connect:', e);
      const message = e instanceof Error ? e.message : 'Failed to connect. Please check your credentials.';
      setError(message);
    } finally {
      setIsConnecting(false);
    }
  };

  if (!connection) return null;

  return (
    <Dialog open={open} onOpenChange={(open) => {
      // 关闭 modal 时清除敏感数据
      if (!open) {
        setPassword('');
      }
      onOpenChange(open);
    }}>
      <DialogContent className="sm:max-w-[500px] bg-theme-bg-panel border-theme-border text-theme-text">
        <DialogHeader>
          <DialogTitle className="text-theme-text">{t('modals.edit_connection.title', { name: connection.name })}</DialogTitle>
          <DialogDescription className="text-theme-text-muted">
            {connection.username}@{connection.host}:{connection.port}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 p-4">
          {error && (
            <div className="bg-red-500/10 border border-red-500/50 text-red-400 px-3 py-2 rounded-sm text-sm">
              {error}
            </div>
          )}

          <div className="space-y-2">
            <Label className="text-theme-text">{t('modals.edit_connection.auth_method')}</Label>
            <RadioGroup 
              value={authType} 
              onValueChange={(v: 'password' | 'key' | 'agent') => setAuthType(v)}
              className="flex space-x-4"
            >
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="password" id="auth-password" className="border-theme-border data-[state=checked]:bg-theme-accent" />
                <Label htmlFor="auth-password" className="text-theme-text">{t('modals.edit_connection.password')}</Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="key" id="auth-key" className="border-theme-border data-[state=checked]:bg-theme-accent" />
                <Label htmlFor="auth-key" className="text-theme-text">{t('modals.edit_connection.ssh_key')}</Label>
              </div>
              <div className="flex items-center space-x-2">
                <RadioGroupItem value="agent" id="auth-agent" className="border-theme-border data-[state=checked]:bg-theme-accent" />
                <Label htmlFor="auth-agent" className="text-theme-text">{t('modals.edit_connection.ssh_agent')}</Label>
              </div>
            </RadioGroup>
          </div>

          {authType === 'password' ? (
            <div className="space-y-2">
              <Label htmlFor="password" className="text-theme-text">{t('modals.edit_connection.password')}</Label>
              <Input
                id="password"
                type="password"
                placeholder={t('modals.edit_connection.password_placeholder')}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="bg-theme-bg-panel border-theme-border text-theme-text focus-visible:ring-theme-accent"
                autoFocus
              />
            </div>
          ) : authType === 'key' ? (
            <>
              <div className="space-y-2">
                <Label htmlFor="key-path" className="text-theme-text">{t('modals.edit_connection.key_path')}</Label>
                <Input
                  id="key-path"
                  placeholder="~/.ssh/id_rsa"
                  value={keyPath}
                  onChange={(e) => setKeyPath(e.target.value)}
                  className="bg-theme-bg-panel border-theme-border text-theme-text focus-visible:ring-theme-accent"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="passphrase" className="text-theme-text">{t('modals.edit_connection.passphrase')}</Label>
                <Input
                  id="passphrase"
                  type="password"
                  placeholder={t('modals.edit_connection.passphrase_placeholder')}
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                  className="bg-theme-bg-panel border-theme-border text-theme-text focus-visible:ring-theme-accent"
                />
              </div>
            </>
          ) : (
            <div className="text-sm text-zinc-400 pt-2 space-y-2">
              <p>{t('modals.edit_connection.agent_desc')}</p>
              <p className="text-xs text-zinc-500">
              {t('modals.edit_connection.agent_hint')}
              </p>
            </div>
          )}

          <div className="space-y-2">
            <Label htmlFor="group" className="text-theme-text">{t('modals.edit_connection.group')}</Label>
            <Select 
              value={group || '__none__'} 
              onValueChange={(v) => setGroup(v === '__none__' ? '' : v)}
            >
              <SelectTrigger className="w-full bg-theme-bg-panel border-theme-border text-theme-text focus:ring-theme-accent">
                <SelectValue placeholder={t('modals.edit_connection.no_group')} />
              </SelectTrigger>
              <SelectContent className="bg-theme-bg-panel border-theme-border text-theme-text">
                <SelectItem value="__none__">{t('modals.edit_connection.no_group')}</SelectItem>
                {groups.map(g => (
                  <SelectItem key={g} value={g}>{g}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button 
            variant="ghost" 
            onClick={() => onOpenChange(false)}
            disabled={isConnecting}
            className="text-theme-text-muted hover:text-theme-text"
          >
            {t('modals.edit_connection.cancel')}
          </Button>
          <Button 
            onClick={handleConnect}
            disabled={isConnecting || (authType === 'password' && !password) || (authType === 'key' && !keyPath)}
            className="bg-theme-accent hover:bg-theme-accent-hover text-white"
          >
            {isConnecting ? t('modals.edit_connection.connecting') : t('modals.edit_connection.connect')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
