import { X, Check, AlertCircle, RotateCcw, History, RefreshCw, Pause, Play } from 'lucide-react';
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Progress } from '../ui/progress';
import { useTransferStore, formatBytes, formatSpeed, calculateSpeed, TransferItem } from '../../store/transferStore';
import { nodeSftpListIncompleteTransfers, nodeSftpResumeTransfer } from '../../lib/api';
import { useNodeState } from '../../hooks/useNodeState';
import { IncompleteTransferInfo } from '../../types';

export const TransferQueue = ({ nodeId }: { nodeId: string }) => {
  const { t } = useTranslation();
  const { getAllTransfers, clearCompleted, cancelTransfer, removeTransfer, addTransfer, pauseTransfer, resumeTransfer } = useTransferStore();
  
  // 🔴 Phase 4: use node readiness instead of connection state
  const { state: nodeState } = useNodeState(nodeId);
  const isConnectionReady = nodeState.readiness === 'ready';

  const items = getAllTransfers();
  const [incompleteTransfers, setIncompleteTransfers] = useState<IncompleteTransferInfo[]>([]);
  const [showIncomplete, setShowIncomplete] = useState(false);
  const [loadingIncomplete, setLoadingIncomplete] = useState(false);

  const activeCount = items.filter(i => i.state === 'active' || i.state === 'pending').length;
  const hasCompleted = items.some(i => i.state === 'completed');
  const hasIncomplete = incompleteTransfers.length > 0;

  // Load incomplete transfers on mount and when session changes
  // 🔴 前端熔断：只有当连接真正 ready 时才加载
  useEffect(() => {
    if (!nodeId) return;
    
    // 🚦 状态门禁：必须等待 node ready 才能请求后端
    if (!isConnectionReady) {
      console.debug(`[TransferQueue] Waiting for node to be ready (current: ${nodeState.readiness})`);
      return;
    }

    const loadIncomplete = async () => {
      setLoadingIncomplete(true);
      try {
        const transfers = await nodeSftpListIncompleteTransfers(nodeId);
        setIncompleteTransfers(transfers);
      } catch (e) {
        const errorMsg = e instanceof Error ? e.message : String(e);

        // 防御性处理：如果是反序列化错误（存储结构版本不兼容），静默忽略
        if (errorMsg.includes('deserialize') || errorMsg.includes('invalid type')) {
          console.debug('[TransferQueue] Storage format incompatible, ignoring old data.');
          setIncompleteTransfers([]);
        }
        // CONNECTION_NOT_FOUND 应该不会发生了（有状态门禁），但保留兜底
        else if (errorMsg.includes('CONNECTION_NOT_FOUND') || errorMsg.includes('NotFound')) {
          console.debug(`[TransferQueue] Node ${nodeId} not found, skipping.`);
          setIncompleteTransfers([]);
        }
        else {
          // 其他错误静默处理，不打扰用户
          console.debug('[TransferQueue] Failed to load incomplete transfers:', e);
        }
      } finally {
        setLoadingIncomplete(false);
      }
    };

    loadIncomplete();
  }, [nodeId, isConnectionReady, nodeState.readiness]);

  const getProgress = (item: TransferItem): number => {
    if (item.size === 0) return 0;
    return Math.round((item.transferred / item.size) * 100);
  };

  const getStatusText = (item: TransferItem): string => {
    switch (item.state) {
      case 'pending': return t('sftp.queue.status_waiting');
      case 'active': return formatSpeed(calculateSpeed(item));
      case 'paused': return t('sftp.queue.status_paused');
      case 'completed': return t('sftp.queue.status_completed');
      case 'cancelled': return t('sftp.queue.status_cancelled');
      case 'error': return item.error || t('sftp.queue.status_error');
      default: return '';
    }
  };

  const handleResumeIncomplete = async (transfer: IncompleteTransferInfo) => {
    if (!transfer.can_resume) return;

    try {
      await nodeSftpResumeTransfer(nodeId, transfer.transfer_id);

      // Add to active transfer queue
      const fileName = transfer.source_path.split('/').pop() || transfer.source_path;
      addTransfer({
        id: transfer.transfer_id,
        nodeId: transfer.session_id,
        name: fileName,
        localPath: transfer.transfer_type === 'Download' ? transfer.destination_path : transfer.source_path,
        remotePath: transfer.transfer_type === 'Upload' ? transfer.destination_path : transfer.source_path,
        direction: transfer.transfer_type.toLowerCase() as 'upload' | 'download',
        size: transfer.total_bytes,
      });

      // Remove from incomplete list
      setIncompleteTransfers(prev => prev.filter(t => t.transfer_id !== transfer.transfer_id));
    } catch (e) {
      console.error('Failed to resume incomplete transfer:', e);
    }
  };

  const handleCancel = async (item: TransferItem) => {
    // If already cancelled/completed/error, just remove from UI
    if (item.state === 'cancelled' || item.state === 'completed' || item.state === 'error') {
      removeTransfer(item.id);
      return;
    }
    
    // Otherwise, cancel the active transfer (cancelTransfer now calls backend API internally)
    try {
      await cancelTransfer(item.id);
    } catch (e) {
      console.error('Failed to cancel transfer:', e);
    }
  };

  const handlePause = async (item: TransferItem) => {
    try {
      await pauseTransfer(item.id);
    } catch (e) {
      console.error('Failed to pause transfer:', e);
    }
  };

  const handleResume = async (item: TransferItem) => {
    try {
      await resumeTransfer(item.id);
    } catch (e) {
      console.error('Failed to resume transfer:', e);
    }
  };

  return (
    <div className="h-48 bg-theme-bg border-t border-theme-border flex flex-col">
      <div className="flex items-center justify-between px-2 py-1 bg-theme-bg-panel border-b border-theme-border">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-zinc-400 uppercase tracking-wide">
            {t('sftp.queue.title')} {activeCount > 0 ? t('sftp.queue.active_count', { count: activeCount }) : ''}
          </span>
          {hasIncomplete && (
            <button
              onClick={() => setShowIncomplete(!showIncomplete)}
              className="text-xs text-oxide-accent hover:underline flex items-center gap-1"
            >
              <History className="h-3 w-3" />
              {t('sftp.queue.incomplete_count', { count: incompleteTransfers.length })}
            </button>
          )}
        </div>
        <div className="flex items-center gap-2">
            {hasCompleted && (
              <Button
                size="sm"
                variant="ghost"
                className="h-6 text-xs px-2"
                onClick={clearCompleted}
              >
                {t('sftp.queue.clear_done')}
              </Button>
            )}
        </div>
      </div>

      {/* Incomplete Transfers Section */}
      {showIncomplete && hasIncomplete && (
        <div className="border-b border-theme-border bg-zinc-900/30">
          <div className="px-2 py-1 text-[10px] text-zinc-500 uppercase tracking-wide">
            {t('sftp.queue.incomplete_title')}
          </div>
          <div className="max-h-32 overflow-y-auto p-2 space-y-1">
            {incompleteTransfers.map(transfer => (
              <div
                key={transfer.transfer_id}
                className="flex items-center gap-2 text-xs p-2 bg-zinc-900/50 rounded-sm border border-yellow-500/30 hover:border-yellow-500/50"
              >
                <div className="w-4 text-center text-yellow-500 font-bold">
                  {transfer.transfer_type === 'Upload' ? '↑' : '↓'}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="truncate text-zinc-300" title={transfer.source_path}>
                    {transfer.source_path.split('/').pop()}
                  </div>
                  <div className="flex items-center gap-2 text-[10px] text-zinc-500">
                    <span>{transfer.transfer_type}</span>
                    <span>•</span>
                    <span>{Math.round(transfer.progress_percent)}%</span>
                    <span>•</span>
                    <span>{formatBytes(transfer.transferred_bytes)} / {formatBytes(transfer.total_bytes)}</span>
                  </div>
                  {transfer.error && (
                    <div className="text-[10px] text-red-400 truncate" title={transfer.error}>
                      {transfer.error}
                    </div>
                  )}
                </div>
                <div className="text-right text-[10px] text-zinc-500">
                  {transfer.status === 'Paused' && t('sftp.queue.status_paused')}
                  {transfer.status === 'Failed' && t('sftp.queue.status_error')}
                </div>
                {transfer.can_resume && (
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-6 w-6 text-yellow-500 hover:text-yellow-400 hover:bg-yellow-500/10"
                    onClick={() => handleResumeIncomplete(transfer)}
                    title={t('sftp.queue.resume_tooltip')}
                  >
                    <RotateCcw className="h-3 w-3" />
                  </Button>
                )}
              </div>
            ))}
            {loadingIncomplete && (
              <div className="flex items-center justify-center py-2 text-xs text-zinc-500">
                <RefreshCw className="h-3 w-3 mr-2 animate-spin" />
                {t('sftp.queue.loading')}
              </div>
            )}
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-2 space-y-2">
         {items.length === 0 ? (
           <div className="flex items-center justify-center h-full text-sm text-zinc-500">
             {t('sftp.queue.empty')}
           </div>
         ) : (
           items.map(item => (
             <div 
               key={item.id} 
               className={`flex items-center gap-3 text-sm p-2 bg-zinc-900/50 rounded-sm border ${
                 item.state === 'error' ? 'border-red-500/50' : 
                 item.state === 'cancelled' ? 'border-yellow-500/30' :
                 'border-transparent hover:border-theme-border'
               }`}
             >
                 <div className="w-4 text-center text-zinc-500 font-bold">
                     {item.direction === 'upload' ? '↑' : '↓'}
                 </div>
                 <div className="w-48 truncate text-zinc-300" title={item.name}>
                     {item.name}
                 </div>
                 <div className="flex-1 flex flex-col gap-1">
                     <Progress value={getProgress(item)} className="h-1.5" />
                     <div className="flex justify-between text-[10px] text-zinc-500">
                       <span>{formatBytes(item.transferred)} / {formatBytes(item.size)}</span>
                       <span>{getProgress(item)}%</span>
                     </div>
                 </div>
                 <div className={`w-24 text-right text-xs font-mono ${
                   item.state === 'error' ? 'text-red-400' : 
                   item.state === 'cancelled' ? 'text-yellow-500' :
                   'text-zinc-500'
                 }`}>
                     {getStatusText(item)}
                 </div>
                 <div className="flex items-center gap-1">
                     {item.state === 'completed' && (
                         <Check className="h-4 w-4 text-green-500" />
                     )}
                     {item.state === 'cancelled' && (
                         <AlertCircle className="h-4 w-4 text-yellow-500" />
                     )}
                     {item.state === 'error' && (
                         <AlertCircle className="h-4 w-4 text-red-400" />
                     )}
                     
                     {/* Pause/Resume button for active transfers */}
                     {(item.state === 'active' || item.state === 'pending') && (
                       <Button 
                         size="icon" 
                         variant="ghost" 
                         className="h-6 w-6 hover:text-yellow-500"
                         onClick={() => handlePause(item)}
                         title={t('sftp.queue.pause_tooltip')}
                       >
                         <Pause className="h-3 w-3" />
                       </Button>
                     )}
                     
                     {item.state === 'paused' && (
                       <Button 
                         size="icon" 
                         variant="ghost" 
                         className="h-6 w-6 hover:text-green-500"
                         onClick={() => handleResume(item)}
                         title={t('sftp.queue.resume_tooltip')}
                       >
                         <Play className="h-3 w-3" />
                       </Button>
                     )}
                     
                     {/* Cancel button (X): cancel during transfer, remove when finished */}
                     <Button 
                       size="icon" 
                       variant="ghost" 
                       className={`h-6 w-6 ${
                         (item.state === 'active' || item.state === 'pending' || item.state === 'paused') 
                         ? 'hover:text-red-400' 
                         : ''
                       }`}
                       onClick={() => handleCancel(item)}
                       title={
                         (item.state === 'active' || item.state === 'pending' || item.state === 'paused')
                         ? t('sftp.queue.cancel_tooltip')
                         : t('sftp.queue.remove_tooltip')
                       }
                     >
                       <X className="h-3 w-3" />
                     </Button>
                 </div>
             </div>
         ))
         )}
      </div>
    </div>
  );
};
