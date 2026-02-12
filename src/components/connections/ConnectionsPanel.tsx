import React, { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { 
  Server, 
  Terminal, 
  FolderOpen, 
  GitFork, 
  RefreshCw, 
  Clock,
  Shield,
  ShieldOff
} from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { Button } from '../ui/button';
import { cn } from '../../lib/utils';
import { SshConnectionInfo, SshConnectionState } from '../../types';
import { useTabBgActive } from '../../hooks/useTabBackground';

// Format connection state
const useFormatState = () => {
  const { t } = useTranslation();
  
  return (state: SshConnectionState): { text: string; color: string } => {
    if (typeof state === 'object' && 'error' in state) {
      return { text: t('connections.state.error', { error: state.error }), color: 'text-red-400' };
    }
    switch (state) {
      case 'connecting':
        return { text: t('connections.state.connecting'), color: 'text-yellow-400' };
      case 'active':
        return { text: t('connections.state.active'), color: 'text-green-400' };
      case 'idle':
        return { text: t('connections.state.idle'), color: 'text-amber-400' };
      case 'disconnecting':
        return { text: t('connections.state.disconnecting'), color: 'text-orange-400' };
      case 'disconnected':
        return { text: t('connections.state.disconnected'), color: 'text-zinc-500' };
      default:
        return { text: String(state), color: 'text-zinc-400' };
    }
  };
};

// Format time
const useFormatTime = () => {
  const { t } = useTranslation();
  
  return (isoString: string): string => {
    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    
    if (diffMins < 1) return t('connections.time.just_now');
    if (diffMins < 60) return t('connections.time.mins_ago', { count: diffMins });
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return t('connections.time.hrs_ago', { count: diffHours });
    return date.toLocaleDateString();
  };
};

// Single Connection Card
const ConnectionCard: React.FC<{
  connection: SshConnectionInfo;
  onToggleKeepAlive: (connectionId: string, keepAlive: boolean) => void;
}> = ({ connection, onToggleKeepAlive }) => {
  const { t } = useTranslation();
  const formatState = useFormatState();
  const formatTime = useFormatTime();
  
  const { text: stateText, color: stateColor } = formatState(connection.state);
  const isIdle = connection.state === 'idle';
  const isActive = connection.state === 'active';
  
  return (
    <div className={cn(
      "border border-theme-border rounded-lg p-4 space-y-3",
      "bg-theme-bg-panel hover:border-zinc-600 transition-colors",
      isIdle && "border-amber-500/30"
    )}>
      {/* Header: Host Info and State */}
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-2">
          <Server className={cn("h-5 w-5", isActive ? "text-green-400" : isIdle ? "text-amber-400" : "text-zinc-500")} />
          <div>
            <div className="font-medium text-sm">
              {connection.username}@{connection.host}:{connection.port}
            </div>
            <div className={cn("text-xs", stateColor)}>
              {stateText}
            </div>
          </div>
        </div>
        
        {/* Keep Alive Button */}
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={() => onToggleKeepAlive(connection.id, !connection.keepAlive)}
          title={connection.keepAlive ? t('connections.keep_alive.disable_tooltip') : t('connections.keep_alive.enable_tooltip')}
        >
          {connection.keepAlive ? (
            <Shield className="h-4 w-4 text-green-400" />
          ) : (
            <ShieldOff className="h-4 w-4 text-zinc-500" />
          )}
        </Button>
      </div>
      
      {/* Stats */}
      <div className="grid grid-cols-3 gap-2 text-xs text-zinc-400">
        <div className="flex items-center gap-1">
          <Terminal className="h-3 w-3" />
          <span>{t('connections.panel.terminals', { count: connection.terminalIds.length })}</span>
        </div>
        <div className="flex items-center gap-1">
          <FolderOpen className="h-3 w-3" />
          <span>{t('connections.panel.sftp', { count: connection.sftpSessionId ? 1 : 0 })}</span>
        </div>
        <div className="flex items-center gap-1">
          <GitFork className="h-3 w-3" />
          <span>{t('connections.panel.forwards', { count: connection.forwardIds.length })}</span>
        </div>
      </div>
      
      {/* Time Info */}
      <div className="flex items-center justify-between text-xs text-zinc-500">
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          <span>{t('connections.panel.created', { time: formatTime(connection.createdAt) })}</span>
        </div>
        {isIdle && (
          <span className="text-amber-400">
            {t('connections.panel.idle_hint', { 
              keepAlive: connection.keepAlive 
                ? t('connections.panel.keep_alive_enabled') 
                : t('connections.panel.disconnect_in') 
            })}
          </span>
        )}
      </div>
    </div>
  );
};

// Connection Management Panel Main Component
export const ConnectionsPanel: React.FC = () => {
  const { t } = useTranslation();
  const bgActive = useTabBgActive('connection_pool');
  const { 
    connections, 
    refreshConnections, 
    setConnectionKeepAlive
  } = useAppStore();
  
  const [loading, setLoading] = React.useState(false);
  
  // Load connection list
  useEffect(() => {
    refreshConnections();
  }, [refreshConnections]);
  
  const handleRefresh = async () => {
    setLoading(true);
    try {
      await refreshConnections();
    } finally {
      setLoading(false);
    }
  };
  
  const handleToggleKeepAlive = async (connectionId: string, keepAlive: boolean) => {
    try {
      await setConnectionKeepAlive(connectionId, keepAlive);
    } catch (error) {
      console.error('Failed to set keep alive:', error);
    }
  };
  
  const connectionList = Array.from(connections.values())
    .filter(conn => conn.state !== 'disconnected');
  
  return (
    <div className={`h-full flex flex-col ${bgActive ? '' : 'bg-theme-bg'}`} data-bg-active={bgActive || undefined}>
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-theme-border bg-theme-bg-panel/50">
        <div>
          <h2 className="text-xl font-semibold text-zinc-100">{t('connections.panel.title')}</h2>
          <p className="text-sm text-zinc-500 mt-1">{t('connections.panel.description')}</p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={handleRefresh}
          disabled={loading}
          className="gap-2"
        >
          <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} />
          {t('connections.panel.refresh')}
        </Button>
      </div>
      
      {/* Connection List */}
      <div className="flex-1 overflow-y-auto p-6">
        {connectionList.length === 0 ? (
          <div className="text-center text-zinc-500 py-16">
            <Server className="h-16 w-16 mx-auto mb-4 opacity-30" />
            <p className="text-lg">{t('connections.panel.no_connections')}</p>
            <p className="text-sm mt-2 opacity-70">{t('connections.panel.no_connections_hint')}</p>
          </div>
        ) : (
          <div className="grid gap-4 max-w-4xl">
            {connectionList.map(conn => (
              <ConnectionCard
                key={conn.id}
                connection={conn}
                onToggleKeepAlive={handleToggleKeepAlive}
              />
            ))}
          </div>
        )}
      </div>
      
      {/* Footer Legend */}
      <div className="px-6 py-4 border-t border-theme-border bg-theme-bg-panel/30 flex items-center gap-6 text-sm text-zinc-500">
        <div className="flex items-center gap-2">
          <Shield className="h-4 w-4 text-green-400" />
          <span>{t('connections.keep_alive.legend_enabled')}</span>
        </div>
        <div className="flex items-center gap-2">
          <ShieldOff className="h-4 w-4 text-zinc-500" />
          <span>{t('connections.keep_alive.legend_disabled')}</span>
        </div>
      </div>
    </div>
  );
};
