import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Play, Square, RefreshCcw, Plus, Trash2, ArrowRight, Pencil, Activity, X, Loader2 } from 'lucide-react';
import { Button } from '../ui/button';
import { Separator } from '../ui/separator';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { RadioGroup, RadioGroupItem } from '../ui/radio-group';
import { Checkbox } from '../ui/checkbox';
import { api } from '../../lib/api';
import { createTypeGuard } from '../../lib/utils';
import { ForwardRule, ForwardType } from '../../types';
import { useToast } from '../../hooks/useToast';
import { useForwardEvents, ForwardStatus as EventForwardStatus } from '../../hooks/useForwardEvents';

// Type guard for ForwardType using const type parameter (TS 5.0+)
const FORWARD_TYPES = ['local', 'remote', 'dynamic'] as const;
const isForwardType = createTypeGuard(FORWARD_TYPES);

interface ForwardStats {
  connection_count: number;
  active_connections: number;
  bytes_sent: number;
  bytes_received: number;
}

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

export const ForwardsView = ({ nodeId }: { nodeId: string }) => {
  const { t } = useTranslation();
  const { toast } = useToast();
  const [forwards, setForwards] = useState<ForwardRule[]>([]);
  const [forwardStats, setForwardStats] = useState<Record<string, ForwardStats>>({});
  const [loading, setLoading] = useState(false);
  const [showNewForm, setShowNewForm] = useState(false);
  const [editingForward, setEditingForward] = useState<ForwardRule | null>(null);

  // New Forward Form State
  const [forwardType, setForwardType] = useState<ForwardType>('local');
  const [bindAddress, setBindAddress] = useState('localhost');
  const [bindPort, setBindPort] = useState('');
  const [targetHost, setTargetHost] = useState('localhost');
  const [targetPort, setTargetPort] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [skipHealthCheck, setSkipHealthCheck] = useState(false);

  // Edit Forward Form State (Independent state to avoid conflict with create form)
  const [editBindAddress, setEditBindAddress] = useState('localhost');
  const [editBindPort, setEditBindPort] = useState('');
  const [editTargetHost, setEditTargetHost] = useState('localhost');
  const [editTargetPort, setEditTargetPort] = useState('');
  const [editError, setEditError] = useState<string | null>(null);

  const fetchForwards = useCallback(async () => {
    try {
      setLoading(true);
      const list = await api.nodeListForwards(nodeId);
      setForwards(list);
      
      // Fetch stats for active forwards
      const statsMap: Record<string, ForwardStats> = {};
      for (const fw of list) {
        if (fw.status === 'active') {
          const stats = await api.nodeGetForwardStats(nodeId, fw.id);
          if (stats) {
            statsMap[fw.id] = stats;
          }
        }
      }
      setForwardStats(statsMap);
    } catch (error) {
      console.error("Failed to list forwards:", error);
    } finally {
      setLoading(false);
    }
  }, [nodeId]);

  // Listen for forward events from backend (death reports, status changes)
  useForwardEvents({
    // No sessionId filter — events are accepted for all sessions and matched by forward ID
    onStatusChanged: useCallback((forwardId: string, status: EventForwardStatus, error?: string) => {
      console.log(`[ForwardsView] Forward ${forwardId} status changed to ${status}`, error);
      
      // Update local state immediately for responsive UI
      setForwards((prev) =>
        prev.map((fw) =>
          fw.id === forwardId
            ? { ...fw, status: status as ForwardRule['status'] }
            : fw
        )
      );

      // Show toast for important status changes
      if (status === 'suspended') {
        toast({
          title: t('forwards.toast.suspended_title'),
          description: t('forwards.toast.suspended_desc'),
          variant: 'warning',
        });
      } else if (status === 'error' && error) {
        toast({
          title: t('forwards.toast.error_title'),
          description: error,
          variant: 'error',
        });
      }
    }, [t, toast]),
    onStatsUpdated: useCallback((forwardId: string, stats: ForwardStats) => {
      setForwardStats((prev) => ({ ...prev, [forwardId]: stats }));
    }, []),
    onSessionSuspended: useCallback((suspendedIds: string[]) => {
      console.log(`[ForwardsView] Session suspended, forwards affected:`, suspendedIds);
      
      // Mark all affected forwards as suspended
      setForwards((prev) =>
        prev.map((fw) =>
          suspendedIds.includes(fw.id)
            ? { ...fw, status: 'suspended' as ForwardRule['status'] }
            : fw
        )
      );

      toast({
        title: t('forwards.toast.session_suspended_title'),
        description: t('forwards.toast.session_suspended_desc', { count: suspendedIds.length }),
        variant: 'warning',
      });
    }, [t, toast]),
  });

  useEffect(() => {
    fetchForwards();
    // Poll every 5 seconds for status updates
    const interval = setInterval(fetchForwards, 5000);
    return () => clearInterval(interval);
  }, [nodeId, fetchForwards]);

  const handleCreateQuick = async (type: 'jupyter' | 'tensorboard' | 'vscode') => {
      try {
          if (type === 'jupyter') {
            await api.nodeForwardJupyter(nodeId, 8888, 8888);
            toast({ title: t('forwards.toast.jupyter_created'), description: t('forwards.toast.jupyter_desc') });
          } else if (type === 'tensorboard') {
            await api.nodeForwardTensorboard(nodeId, 6006, 6006);
            toast({ title: t('forwards.toast.tensorboard_created'), description: t('forwards.toast.tensorboard_desc') });
          } else if (type === 'vscode') {
            await api.nodeForwardVscode(nodeId, 8080, 8080);
            toast({ title: t('forwards.toast.vscode_created'), description: t('forwards.toast.vscode_desc') });
          }
          fetchForwards();
      } catch (e) {
          console.error(e);
          toast({ 
            title: t('forwards.toast.create_failed'), 
            description: e instanceof Error ? e.message : String(e),
            variant: 'error'
          });
      }
  };

  const handleCreateForward = async () => {
      setCreateError(null);
      if (!bindPort || (forwardType !== 'dynamic' && !targetPort)) {
          setCreateError(t('forwards.form.port_required'));
          return;
      }

      setIsCreating(true);
      try {
          const response = await api.nodeCreateForward({
              node_id: nodeId,
              forward_type: forwardType,
              bind_address: bindAddress,
              bind_port: parseInt(bindPort),
              target_host: forwardType === 'dynamic' ? '0.0.0.0' : targetHost,
              target_port: forwardType === 'dynamic' ? 0 : parseInt(targetPort),
              check_health: !skipHealthCheck
          });
          
          // Check response for errors
          if (response && !response.success && response.error) {
              setCreateError(response.error);
              setIsCreating(false);
              return;
          }
          
          setShowNewForm(false);
          setBindPort('');
          setTargetPort('');
          setSkipHealthCheck(false);
          fetchForwards();
      } catch (e: unknown) {
          setCreateError(e instanceof Error ? e.message : String(e));
      } finally {
          setIsCreating(false);
      }
  };

  return (
    <div className="h-full w-full bg-theme-bg p-4 overflow-y-auto">
      <div className="max-w-4xl mx-auto space-y-6">
        
        {/* Quick Actions */}
        <div className="space-y-2">
           <h3 className="text-sm font-medium text-zinc-400 uppercase tracking-wide">{t('forwards.quick.title')}</h3>
           <div className="flex gap-2">
             <Button variant="secondary" className="gap-2" onClick={() => handleCreateQuick('jupyter')}>
                <span className="w-2 h-2 rounded-full bg-orange-500" /> {t('forwards.quick.jupyter')}
             </Button>
             <Button variant="secondary" className="gap-2" onClick={() => handleCreateQuick('tensorboard')}>
                <span className="w-2 h-2 rounded-full bg-blue-500" /> {t('forwards.quick.tensorboard')}
             </Button>
             <Button variant="secondary" className="gap-2" onClick={() => handleCreateQuick('vscode')}>
                <span className="w-2 h-2 rounded-full bg-cyan-500" /> {t('forwards.quick.vscode')}
             </Button>
           </div>
        </div>

        <Separator />

        {/* Active Forwards Table */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-zinc-400 uppercase tracking-wide">{t('forwards.table.title')}</h3>
            <div className="flex gap-2">
                <Button size="sm" variant="ghost" onClick={fetchForwards} disabled={loading}>
                    <RefreshCcw className={`h-3 w-3 ${loading ? 'animate-spin' : ''}`} />
                </Button>
                <Button 
                    size="sm" 
                    className="gap-1" 
                    variant={showNewForm ? "secondary" : "default"}
                    onClick={() => setShowNewForm(!showNewForm)}
                >
                    <Plus className="h-3 w-3" /> {t('forwards.actions.new_forward')}
                </Button>
            </div>
          </div>

          <div className="border border-theme-border rounded-sm overflow-hidden min-h-[100px] bg-theme-bg-panel/50">
             <table className="w-full text-sm text-left">
               <thead className="bg-theme-bg-panel text-zinc-500 border-b border-theme-border">
                 <tr>
                   <th className="px-4 py-2 font-medium">{t('forwards.table.type')}</th>
                   <th className="px-4 py-2 font-medium">{t('forwards.table.local_address')}</th>
                   <th className="px-4 py-2 font-medium">{t('forwards.table.remote_address')}</th>
                   <th className="px-4 py-2 font-medium">{t('forwards.table.status')}</th>
                   <th className="px-4 py-2 font-medium text-right">{t('forwards.table.actions')}</th>
                 </tr>
               </thead>
               <tbody className="divide-y divide-oxide-border bg-zinc-950/50">
                 {forwards.length === 0 ? (
                     <tr>
                         <td colSpan={5} className="px-4 py-8 text-center text-zinc-500">
                             {t('forwards.table.no_forwards')}
                         </td>
                     </tr>
                 ) : (
                     forwards.map(fw => (
                  <tr key={fw.id} className="group hover:bg-zinc-900 transition-colors">
                    <td className="px-4 py-2">
                       <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium 
                         ${fw.forward_type === 'local' ? 'bg-blue-900/30 text-blue-400' : 
                           fw.forward_type === 'remote' ? 'bg-purple-900/30 text-purple-400' : 
                           'bg-yellow-900/30 text-yellow-400'}`}>
                         {fw.forward_type}
                       </span>
                    </td>
                    <td className="px-4 py-2 font-mono text-zinc-300">
                        {fw.forward_type === 'remote' ? `${fw.target_host}:${fw.target_port}` : `${fw.bind_address}:${fw.bind_port}`}
                    </td>
                    <td className="px-4 py-2 font-mono text-zinc-300">
                        {fw.forward_type === 'remote' ? `${fw.bind_address}:${fw.bind_port}` : `${fw.target_host}:${fw.target_port}`}
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex items-center gap-1.5">
                        <div className={`w-2 h-2 rounded-full 
                          ${fw.status === 'active' ? 'bg-green-500' : 
                            fw.status === 'stopped' ? 'bg-zinc-500' : 
                            fw.status === 'suspended' ? 'bg-orange-500 animate-pulse' : 'bg-red-500'}`} />
                        <span className={`capitalize ${fw.status === 'suspended' ? 'text-orange-400' : 'text-zinc-400'}`}>
                          {fw.status === 'suspended' ? t('forwards.status.suspended') : fw.status}
                        </span>
                        {/* Show stats for active forwards */}
                        {fw.status === 'active' && forwardStats[fw.id] && (
                          <span className="ml-2 text-xs text-zinc-500 flex items-center gap-1">
                            <Activity className="h-3 w-3" />
                            {forwardStats[fw.id].active_connections}/{forwardStats[fw.id].connection_count}
                            <span className="text-zinc-600">|</span>
                            ↑{formatBytes(forwardStats[fw.id].bytes_sent)} 
                            ↓{formatBytes(forwardStats[fw.id].bytes_received)}
                          </span>
                        )}
                        {/* Show hint for suspended forwards */}
                        {fw.status === 'suspended' && (
                          <span className="ml-2 text-xs text-orange-500/70">
                            {t('forwards.status.suspended_hint')}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-4 py-2 text-right">
                      <div className="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        {fw.status === 'active' ? (
                          // Active forward: show Stop button
                          <Button 
                            size="icon" 
                            variant="ghost" 
                            className="h-7 w-7 text-zinc-400 hover:text-yellow-400"
                            title={t('forwards.actions.stop')}
                            onClick={() => api.nodeStopForward(nodeId, fw.id).then(fetchForwards)}
                          >
                            <Square className="h-3 w-3 fill-current" />
                          </Button>
                        ) : fw.status === 'suspended' ? (
                          // Suspended forward: show hint that it will auto-recover
                          <span className="text-xs text-orange-400/70 px-2">
                            {t('forwards.actions.will_recover')}
                          </span>
                        ) : (
                          // Stopped forward: show Restart and Edit buttons
                          <>
                            <Button 
                              size="icon" 
                              variant="ghost" 
                              className="h-7 w-7 text-zinc-400 hover:text-green-400"
                              title={t('forwards.actions.restart')}
                              onClick={() => api.nodeRestartForward(nodeId, fw.id).then(fetchForwards)}
                            >
                              <Play className="h-3 w-3 fill-current" />
                            </Button>
                            <Button 
                              size="icon" 
                              variant="ghost" 
                              className="h-7 w-7 text-zinc-400 hover:text-blue-400"
                              title={t('forwards.actions.edit')}
                              onClick={() => {
                                setEditingForward(fw);
                                // 使用独立的编辑状态，不影响创建表单
                                setEditBindAddress(fw.bind_address);
                                setEditBindPort(fw.bind_port.toString());
                                setEditTargetHost(fw.target_host);
                                setEditTargetPort(fw.target_port.toString());
                                setEditError(null);
                              }}
                            >
                              <Pencil className="h-3 w-3" />
                            </Button>
                          </>
                        )}
                        {/* Delete button - always available */}
                        <Button 
                          size="icon" 
                          variant="ghost" 
                          className="h-7 w-7 text-zinc-400 hover:text-red-400"
                          title={t('forwards.actions.delete')}
                          onClick={() => api.nodeDeleteForward(nodeId, fw.id).then(fetchForwards)}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))) }
               </tbody>
             </table>
          </div>
        </div>

        {/* New Forward Form */}
        {showNewForm && (
            <div className="border border-theme-border rounded-sm bg-theme-bg-panel/30 p-4 space-y-4 animate-in fade-in slide-in-from-top-2">
                <div className="flex items-center justify-between">
                    <h3 className="text-sm font-medium text-zinc-300">{t('forwards.form.new_title')}</h3>
                    <Button variant="ghost" size="sm" onClick={() => setShowNewForm(false)}>{t('forwards.form.cancel')}</Button>
                </div>
                
                <RadioGroup value={forwardType} onValueChange={(v) => { if (isForwardType(v)) setForwardType(v); }} className="flex gap-4">
                    <div className="flex items-center space-x-2">
                        <RadioGroupItem value="local" id="r-local" />
                        <Label htmlFor="r-local">{t('forwards.form.type_local')}</Label>
                    </div>
                    <div className="flex items-center space-x-2">
                        <RadioGroupItem value="remote" id="r-remote" />
                        <Label htmlFor="r-remote">{t('forwards.form.type_remote')}</Label>
                    </div>
                    <div className="flex items-center space-x-2">
                        <RadioGroupItem value="dynamic" id="r-dynamic" />
                        <Label htmlFor="r-dynamic">{t('forwards.form.type_dynamic')}</Label>
                    </div>
                </RadioGroup>

                <div className="flex items-center gap-4 p-4 bg-zinc-950/50 rounded-sm border border-theme-border/50">
                    {/* Left Side (Source) */}
                    <div className="flex-1 space-y-2">
                        <Label className="text-xs">{forwardType === 'remote' ? t('forwards.form.remote_server') : t('forwards.form.local_client')}</Label>
                        <div className="flex gap-2">
                             <Input 
                                placeholder={t('forwards.form.host_placeholder')} 
                                value={forwardType === 'remote' ? bindAddress : bindAddress}
                                onChange={(e) => setBindAddress(e.target.value)}
                                className="font-mono"
                             />
                             <Input 
                                placeholder={t('forwards.form.port_placeholder')} 
                                value={bindPort}
                                onChange={(e) => setBindPort(e.target.value)}
                                className="w-24 font-mono"
                             />
                        </div>
                    </div>

                    {/* Arrow */}
                    <div className="pt-6 text-zinc-500">
                        <ArrowRight className="h-5 w-5" />
                    </div>

                    {/* Right Side (Target) */}
                    {forwardType === 'dynamic' ? (
                        <div className="flex-1 pt-6 text-sm text-zinc-500 italic text-center">
                            {t('forwards.form.socks5_mode')}
                        </div>
                    ) : (
                        <div className="flex-1 space-y-2">
                            <Label className="text-xs">{forwardType === 'remote' ? t('forwards.form.local_client') : t('forwards.form.remote_server')}</Label>
                            <div className="flex gap-2">
                                <Input 
                                    placeholder={t('forwards.form.host_placeholder')} 
                                    value={targetHost}
                                    onChange={(e) => setTargetHost(e.target.value)}
                                    className="font-mono"
                                />
                                <Input 
                                    placeholder={t('forwards.form.port_placeholder')} 
                                    value={targetPort}
                                    onChange={(e) => setTargetPort(e.target.value)}
                                    className="w-24 font-mono"
                                />
                            </div>
                        </div>
                    )}
                </div>
                
                {/* Skip health check option */}
                {forwardType !== 'dynamic' && (
                    <div className="flex items-center space-x-2 px-2">
                        <Checkbox 
                            id="skip-health"
                            checked={skipHealthCheck}
                            onCheckedChange={(checked) => { if (typeof checked === 'boolean') setSkipHealthCheck(checked); }}
                        />
                        <Label 
                            htmlFor="skip-health" 
                            className="text-xs text-zinc-400 cursor-pointer"
                        >
                            {t('forwards.form.skip_check')}
                        </Label>
                    </div>
                )}
                
                {createError && (
                    <div className="border border-red-900/50 bg-red-950/30 rounded-sm p-3 space-y-2">
                        <div className="flex items-start gap-2">
                            <span className="text-red-400 text-xs font-medium">⚠ Error</span>
                        </div>
                        <div className="text-xs text-zinc-300 whitespace-pre-wrap font-mono">
                            {createError}
                        </div>
                    </div>
                )}

                <div className="flex justify-end gap-2">
                    {isCreating && (
                        <div className="flex items-center gap-2 text-xs text-zinc-400 mr-auto">
                            <Loader2 className="h-3 w-3 animate-spin" />
                            {skipHealthCheck ? t('forwards.form.creating') : t('forwards.form.checking_port')}
                        </div>
                    )}
                    <Button onClick={handleCreateForward} disabled={isCreating}>
                        {isCreating ? t('forwards.form.creating') : t('forwards.form.create_forward')}
                    </Button>
                </div>
            </div>
        )}

        {/* Edit Forward Modal */}
        {editingForward && (
            <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
                <div className="bg-theme-bg-panel border border-theme-border rounded-lg p-6 w-[500px] space-y-4 animate-in fade-in zoom-in-95">
                    <div className="flex items-center justify-between">
                        <h3 className="text-sm font-medium text-zinc-300">{t('forwards.form.edit_title')}</h3>
                        <Button 
                            variant="ghost" 
                            size="icon"
                            className="h-6 w-6"
                            onClick={() => setEditingForward(null)}
                        >
                            <X className="h-4 w-4" />
                        </Button>
                    </div>
                    
                    <div className="text-xs text-zinc-500">
                        {t('forwards.form.type')}: <span className="text-zinc-400 capitalize">{editingForward.forward_type}</span>
                        <span className="mx-2">|</span>
                        ID: <span className="text-zinc-400 font-mono">{editingForward.id.slice(0, 8)}...</span>
                    </div>

                    <div className="flex items-center gap-4 p-4 bg-zinc-950/50 rounded-sm border border-theme-border/50">
                        <div className="flex-1 space-y-2">
                            <Label className="text-xs">{t('forwards.form.bind_address')}</Label>
                            <div className="flex gap-2">
                                <Input 
                                    placeholder={t('forwards.form.host_placeholder')} 
                                    value={editBindAddress}
                                    onChange={(e) => setEditBindAddress(e.target.value)}
                                    className="font-mono"
                                />
                                <Input 
                                    placeholder={t('forwards.form.port_placeholder')} 
                                    value={editBindPort}
                                    onChange={(e) => setEditBindPort(e.target.value)}
                                    className="w-24 font-mono"
                                />
                            </div>
                        </div>

                        <div className="pt-6 text-zinc-500">
                            <ArrowRight className="h-5 w-5" />
                        </div>

                        {editingForward.forward_type !== 'dynamic' && (
                            <div className="flex-1 space-y-2">
                                <Label className="text-xs">{t('forwards.form.target_address')}</Label>
                                <div className="flex gap-2">
                                    <Input 
                                        placeholder={t('forwards.form.host_placeholder')} 
                                        value={editTargetHost}
                                        onChange={(e) => setEditTargetHost(e.target.value)}
                                        className="font-mono"
                                    />
                                    <Input 
                                        placeholder={t('forwards.form.port_placeholder')} 
                                        value={editTargetPort}
                                        onChange={(e) => setEditTargetPort(e.target.value)}
                                        className="w-24 font-mono"
                                    />
                                </div>
                            </div>
                        )}
                    </div>

                    {editError && (
                        <div className="text-red-400 text-xs">{editError}</div>
                    )}

                    <div className="flex justify-end gap-2">
                        <Button variant="ghost" onClick={() => setEditingForward(null)}>{t('forwards.form.cancel')}</Button>
                        <Button onClick={async () => {
                            setEditError(null);
                            try {
                                await api.nodeUpdateForward({
                                    node_id: nodeId,
                                    forward_id: editingForward.id,
                                    bind_address: editBindAddress,
                                    bind_port: parseInt(editBindPort),
                                    target_host: editTargetHost,
                                    target_port: parseInt(editTargetPort),
                                });
                                setEditingForward(null);
                                fetchForwards();
                            } catch (e: unknown) {
                                setEditError(e instanceof Error ? e.message : String(e));
                            }
                        }}>
                            {t('forwards.form.save_changes')}
                        </Button>
                    </div>
                </div>
            </div>
        )}
      </div>
    </div>
  );
};