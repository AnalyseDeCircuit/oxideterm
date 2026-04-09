// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

// src/components/ide/IdeStatusBar.tsx
import { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useIdeStore, useIdeProject, useIdeActiveTab, useIdeDirtyCount } from '../../store/ideStore';
import { GitBranch, Cpu, HardDrive, Trash2, Rocket, Loader2, Info, ExternalLink, Columns2, X } from 'lucide-react';
import { useAgentStatus } from './hooks/useAgentStatus';
import { useConfirm } from '../../hooks/useConfirm';
import * as agentService from '../../lib/agentService';
import { cn } from '../../lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

/** GitHub repo URL for agent build instructions for unsupported architectures. */
const AGENT_DOWNLOAD_BASE_URL = 'https://github.com/AnalyseDeCircuit/oxideterm/blob/main/agents/README.md';

export function IdeStatusBar() {
  const { t } = useTranslation();
  const project = useIdeProject();
  const activeTab = useIdeActiveTab();
  const dirtyCount = useIdeDirtyCount();
  const nodeId = useIdeStore(state => state.nodeId);
  const splitDirection = useIdeStore(state => state.splitDirection);
  const { splitEditor, closeSplit } = useIdeStore();
  const { mode, label, status, refresh } = useAgentStatus(nodeId ?? undefined);
  const { confirm, ConfirmDialog } = useConfirm();
  const [removing, setRemoving] = useState(false);
  const [deploying, setDeploying] = useState(false);
  
  const handleRemoveAgent = useCallback(async () => {
    if (!nodeId) return;
    const yes = await confirm({
      title: t('ide.agent_remove_confirm_title', 'Remove Remote Agent?'),
      description: t('ide.agent_remove_confirm_desc', 'This will stop the agent process and delete the binary from the remote host. IDE mode will fall back to SFTP.'),
      confirmLabel: t('ide.agent_remove_confirm_btn', 'Remove'),
      variant: 'danger',
    });
    if (!yes) return;

    setRemoving(true);
    try {
      await agentService.removeAgent(nodeId);
      refresh();
    } catch (err) {
      console.error('[IdeStatusBar] Failed to remove agent:', err);
    } finally {
      setRemoving(false);
    }
  }, [nodeId, confirm, t, refresh]);

  const handleDeployAgent = useCallback(async () => {
    if (!nodeId) return;
    setDeploying(true);
    try {
      await agentService.ensureAgent(nodeId);
      refresh();
    } catch (err) {
      console.error('[IdeStatusBar] Failed to deploy agent:', err);
    } finally {
      setDeploying(false);
    }
  }, [nodeId, refresh]);

  // Extract version info from status
  const agentVersion = status?.type === 'ready'
    ? `v${(status as { type: 'ready'; version: string }).version}`
    : null;
  
  return (
    <div className="h-6 bg-theme-bg-panel border-t border-theme-border flex items-center px-3 text-xs text-theme-text-muted">
      {/* Agent/SFTP 模式指示器 — 可点击下拉菜单 */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button className={cn(
            "flex items-center gap-1 mr-4 px-1.5 py-0.5 rounded hover:bg-theme-bg-hover transition-colors cursor-pointer",
            mode === 'agent' && "text-emerald-400",
            mode === 'sftp' && "text-theme-text-muted",
            mode === 'manual-upload' && "text-amber-400",
            mode === 'manual-update' && "text-amber-400",
            mode === 'deploying' && "text-amber-400",
            mode === 'checking' && "text-theme-text-muted opacity-50",
          )}>
            {mode === 'agent' ? (
              <Cpu className="w-3 h-3" />
            ) : (
              <HardDrive className="w-3 h-3" />
            )}
            <span>{label}</span>
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="min-w-[180px]">
          {/* Agent 状态描述 */}
          <div className="px-2 py-1.5 text-xs text-theme-text-muted">
            {mode === 'agent' && agentVersion && (
              <span>{t('ide.agent_status_ready', 'Agent active')} ({agentVersion})</span>
            )}
            {mode === 'sftp' && (
              <span>{t('ide.agent_status_sftp', 'SFTP mode (no agent)')}</span>
            )}
            {mode === 'manual-upload' && (
              <span>{t('ide.agent_status_manual_upload', 'Manual upload required')}</span>
            )}
            {mode === 'manual-update' && (
              <span>{t('ide.agent_status_manual_update', 'Manual update required')}</span>
            )}
            {mode === 'deploying' && (
              <span>{t('ide.agent_status_deploying', 'Deploying agent…')}</span>
            )}
            {mode === 'checking' && (
              <span>{t('ide.agent_status_checking', 'Checking agent…')}</span>
            )}
          </div>
          <DropdownMenuSeparator />

          {/* Manual upload instructions */}
          {mode === 'manual-upload' && status?.type === 'manualUploadRequired' && (
            <>
              <div className="px-2 py-2 text-xs text-theme-text-muted max-w-[300px]">
                <div className="flex items-start gap-1.5 mb-2">
                  <Info className="w-3.5 h-3.5 text-amber-400 flex-shrink-0 mt-0.5" />
                  <span>{t('ide.agent_manual_upload_hint', 'Unsupported architecture. You need to build the agent from source yourself and upload it to the remote host.')}</span>
                </div>
                
                {/* Download link */}
                <a
                  href={AGENT_DOWNLOAD_BASE_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1.5 text-theme-accent hover:underline mb-2"
                >
                  <ExternalLink className="w-3 h-3" />
                  {t('ide.agent_download_link', 'Build Instructions')}
                </a>
                
                {/* Remote path */}
                <div className="text-[10px] opacity-70 mb-1">
                  {t('ide.agent_upload_to', 'Upload to:')}
                </div>
                <code className="block bg-theme-bg-hover px-1.5 py-1 rounded text-[10px] font-mono break-all">
                  {status.remotePath}
                </code>
                <div className="mt-1.5 text-[10px] opacity-70">
                  {t('ide.agent_manual_upload_arch', 'Architecture: {{arch}}', { arch: status.arch })}
                </div>
              </div>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={handleDeployAgent}
                disabled={deploying}
                className="gap-2"
              >
                {deploying ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Rocket className="w-3.5 h-3.5" />
                )}
                {t('ide.agent_retry_btn', 'Retry Deploy')}
              </DropdownMenuItem>
            </>
          )}

          {mode === 'manual-update' && status?.type === 'manualUpdateRequired' && (
            <>
              <div className="px-2 py-2 text-xs text-theme-text-muted max-w-[300px]">
                <div className="flex items-start gap-1.5 mb-2">
                  <Info className="w-3.5 h-3.5 text-amber-400 flex-shrink-0 mt-0.5" />
                  <span>{t('ide.agent_manual_update_hint', 'An incompatible agent was detected. You need to build a compatible version yourself and replace the remote file.')}</span>
                </div>

                <a
                  href={AGENT_DOWNLOAD_BASE_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1.5 text-theme-accent hover:underline mb-2"
                >
                  <ExternalLink className="w-3 h-3" />
                  {t('ide.agent_download_link', 'Build Instructions')}
                </a>

                <div className="text-[10px] opacity-70 mb-1">
                  {t('ide.agent_upload_to', 'Upload to:')}
                </div>
                <code className="block bg-theme-bg-hover px-1.5 py-1 rounded text-[10px] font-mono break-all">
                  {status.remotePath}
                </code>
                <div className="mt-1.5 text-[10px] opacity-70">
                  {t('ide.agent_manual_upload_arch', 'Architecture: {{arch}}', { arch: status.arch })}
                </div>
                <div className="mt-1 text-[10px] opacity-70">
                  {t('ide.agent_manual_update_current_agent_version', 'Agent version: {{version}}', {
                    version: status.currentAgentVersion,
                  })}
                </div>
                <div className="mt-1 text-[10px] opacity-70">
                  {t('ide.agent_manual_update_current_compatibility_version', 'Compatibility version: {{version}}', {
                    version: status.currentCompatibilityVersion,
                  })}
                </div>
                <div className="mt-1 text-[10px] opacity-70">
                  {t('ide.agent_manual_update_expected_compatibility_version', 'Required compatibility version: {{version}}', {
                    version: status.expectedCompatibilityVersion,
                  })}
                </div>
              </div>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={handleDeployAgent}
                disabled={deploying}
                className="gap-2"
              >
                {deploying ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Rocket className="w-3.5 h-3.5" />
                )}
                {t('ide.agent_retry_btn', 'Retry Deploy')}
              </DropdownMenuItem>
            </>
          )}

          {/* 部署 Agent（仅 SFTP 模式下显示） */}
          {mode === 'sftp' && (
            <DropdownMenuItem
              onClick={handleDeployAgent}
              disabled={deploying}
              className="gap-2"
            >
              {deploying ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Rocket className="w-3.5 h-3.5" />
              )}
              {t('ide.agent_deploy_btn', 'Deploy Agent')}
            </DropdownMenuItem>
          )}

          {/* 删除 Agent（仅 Agent 运行时显示） */}
          {mode === 'agent' && (
            <DropdownMenuItem
              onClick={handleRemoveAgent}
              disabled={removing}
              className="gap-2 text-red-400 focus:text-red-400"
            >
              {removing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Trash2 className="w-3.5 h-3.5" />
              )}
              {t('ide.agent_remove_btn', 'Remove Agent')}
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Git 分支 */}
      {project?.isGitRepo && project.gitBranch && (
        <div className="flex items-center gap-1 mr-4">
          <GitBranch className="w-3 h-3" />
          <span>{project.gitBranch}</span>
        </div>
      )}
      
      {/* 光标位置 */}
      {activeTab?.cursor && (
        <span className="mr-4">
          Ln {activeTab.cursor.line}, Col {activeTab.cursor.col}
        </span>
      )}
      
      {/* 语言 */}
      {activeTab && (
        <span className="mr-4">{activeTab.language}</span>
      )}
      
      {/* 分栏编辑器 */}
      <button
        className={cn(
          'flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-theme-bg-hover transition-colors mr-2',
          splitDirection && 'text-theme-accent',
        )}
        onClick={() => splitDirection ? closeSplit() : splitEditor()}
        title={splitDirection
          ? t('ide.close_split', 'Close split editor')
          : t('ide.split_editor', 'Split editor')
        }
      >
        {splitDirection ? (
          <X className="w-3 h-3" />
        ) : (
          <Columns2 className="w-3 h-3" />
        )}
      </button>

      {/* 未保存文件数 */}
      {dirtyCount > 0 && (
        <span className="ml-auto text-theme-accent">
          {dirtyCount} unsaved
        </span>
      )}
      
      {/* 确认对话框 */}
      {ConfirmDialog}
    </div>
  );
}
