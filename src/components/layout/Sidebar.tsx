import React, { useEffect, useState, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Terminal,
  Folder,
  FolderOpen,
  Settings,
  Plus,
  ChevronRight,
  Server,
  Search,
  ListChecks,
  Link2,
  Activity,
  Network,
  Database,
  Sparkles,
  Square,
  PanelLeftClose,
  PanelLeft,
  LayoutList,
  Puzzle,
  Monitor,
} from 'lucide-react';
import { platform } from '../../lib/platform';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { usePluginStore } from '../../store/pluginStore';
import { resolvePluginIcon } from '../../lib/plugin/pluginIconResolver';
import { useToast } from '../../hooks/useToast';
import { useConfirm } from '../../hooks/useConfirm';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';
import { EditConnectionModal } from '../modals/EditConnectionModal';
import { SessionTree } from '../sessions/SessionTree';
import { Breadcrumb } from '../sessions/Breadcrumb';
import { FocusedNodeList } from '../sessions/FocusedNodeList';
import { DrillDownDialog } from '../modals/DrillDownDialog';
import { SavePathAsPresetDialog } from '../modals/SavePathAsPresetDialog';
import { AddRootNodeDialog } from '../modals/AddRootNodeDialog';
import { api } from '../../lib/api';
import { connectToSaved } from '../../lib/connectToSaved';

import { PluginSidebarRenderer } from '../plugin/PluginSidebarRenderer';

export const Sidebar = () => {
  const { t } = useTranslation();

  // Sidebar state from settingsStore (for reactivity)
  const sidebarCollapsed = useSettingsStore((s) => s.settings.sidebarUI.collapsed);
  const sidebarActiveSection = useSettingsStore((s) => s.settings.sidebarUI.activeSection);
  const sidebarWidth = useSettingsStore((s) => s.settings.sidebarUI.width);
  const aiSidebarCollapsed = useSettingsStore((s) => s.settings.sidebarUI.aiSidebarCollapsed);
  const { setSidebarWidth, toggleSidebar, toggleAiSidebar } = useSettingsStore();

  // Resize state
  const [isResizing, setIsResizing] = useState(false);
  const sidebarRef = useRef<HTMLDivElement>(null);

  const setSidebarSection = useAppStore((s) => s.setSidebarSection);
  const sessions = useAppStore((s) => s.sessions);
  const connections = useAppStore((s) => s.connections);
  const toggleModal = useAppStore((s) => s.toggleModal);
  const createTab = useAppStore((s) => s.createTab);
  const closeTab = useAppStore((s) => s.closeTab);
  const tabs = useAppStore((s) => s.tabs);
  const activeTabId = useAppStore((s) => s.activeTabId);
  const setActiveTab = useAppStore((s) => s.setActiveTab);
  const savedConnections = useAppStore((s) => s.savedConnections);
  const loadSavedConnections = useAppStore((s) => s.loadSavedConnections);
  const loadGroups = useAppStore((s) => s.loadGroups);
  const modals = useAppStore((s) => s.modals);
  const editingConnection = useAppStore((s) => s.editingConnection);
  const refreshConnections = useAppStore((s) => s.refreshConnections);
  const openConnectionEditor = useAppStore((s) => s.openConnectionEditor);

  // SessionTree store
  const treeNodes = useSessionTreeStore((s) => s.nodes);
  const selectedNodeId = useSessionTreeStore((s) => s.selectedNodeId);
  const getFocusedNodeId = useSessionTreeStore((s) => s.getFocusedNodeId);
  const fetchTree = useSessionTreeStore((s) => s.fetchTree);
  const selectNode = useSessionTreeStore((s) => s.selectNode);
  const toggleExpand = useSessionTreeStore((s) => s.toggleExpand);
  const removeNode = useSessionTreeStore((s) => s.removeNode);
  const getNode = useSessionTreeStore((s) => s.getNode);
  const createTerminalForNode = useSessionTreeStore((s) => s.createTerminalForNode);
  const closeTerminalForNode = useSessionTreeStore((s) => s.closeTerminalForNode);
  const connectNode = useSessionTreeStore((s) => s.connectNode);
  const disconnectNode = useSessionTreeStore((s) => s.disconnectNode);
  const setFocusedNode = useSessionTreeStore((s) => s.setFocusedNode);
  const getBreadcrumbPath = useSessionTreeStore((s) => s.getBreadcrumbPath);
  const getVisibleNodes = useSessionTreeStore((s) => s.getVisibleNodes);
  const enterNode = useSessionTreeStore((s) => s.enterNode);

  const [savedSearchQuery, setSavedSearchQuery] = useState('');

  // 视图模式：'tree' = 传统树形视图, 'focus' = 面包屑+聚焦模式
  const [viewMode, setViewMode] = useState<'tree' | 'focus'>('tree');

  // SessionTree 对话框状态
  const [drillDownDialog, setDrillDownDialog] = useState<{ open: boolean; parentId: string; parentHost: string }>({
    open: false,
    parentId: '',
    parentHost: '',
  });
  const [savePresetDialog, setSavePresetDialog] = useState<{ open: boolean; nodeId: string }>({
    open: false,
    nodeId: '',
  });
  const [addRootNodeOpen, setAddRootNodeOpen] = useState(false);

  // Local terminal store
  const createLocalTerminal = useLocalTerminalStore((s) => s.createTerminal);
  const localTerminals = useLocalTerminalStore((s) => s.terminals);

  // Toast hook (需要在所有使用 toast 的 useCallback 之前声明)
  const { toast } = useToast();
  const { confirm, ConfirmDialog } = useConfirm();

  // Handle creating a new local terminal
  const handleNewLocalTerminal = useCallback(async () => {
    try {
      const info = await createLocalTerminal();
      // Open a local_terminal tab
      createTab('local_terminal', info.id);
    } catch (err) {
      console.error('Failed to create local terminal:', err);
    }
  }, [createLocalTerminal, createTab]);

  // ========== Resize Handling ==========
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;

      // Calculate new width based on mouse position
      const newWidth = e.clientX;
      setSidebarWidth(newWidth);
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      // Prevent text selection during resize
      document.body.style.userSelect = 'none';
      document.body.style.cursor = 'col-resize';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.userSelect = '';
      document.body.style.cursor = '';
    };
  }, [isResizing, setSidebarWidth]);

  // Load saved connections and groups on mount
  useEffect(() => {
    loadSavedConnections();
    loadGroups();
  }, []);

  // Load session tree on mount
  useEffect(() => {
    fetchTree();
  }, [fetchTree]);

  // ========== SessionTree 回调函数 ==========
  const handleTreeDrillDown = useCallback((parentId: string) => {
    const node = getNode(parentId);
    if (node) {
      setDrillDownDialog({
        open: true,
        parentId,
        parentHost: node.displayName || `${node.username}@${node.host}`,
      });
    }
  }, [getNode]);

  /**
   * Phase 3.3: 使用 connectNodeWithAncestors 线性连接器
   * 
   * 执行流程：
   * 1. 通过 sessionTreeStore.connectNodeWithAncestors 建立连接链
   * 2. 连接成功后创建终端会话
   * 3. 关联终端到节点
   * 4. 打开终端 Tab
   * 
   * 错误处理：
   * - CHAIN_LOCK_BUSY: 提示用户稍后重试
   * - NODE_LOCK_BUSY: 提示节点正在连接中
   * - CONNECTION_CHAIN_FAILED: 显示失败节点信息
   */
  const handleTreeConnect = useCallback(async (nodeId: string) => {
    const { connectNodeWithAncestors, isNodeConnecting, isConnectingChain } = useSessionTreeStore.getState();
    
    // 前端预检查（避免不必要的请求）
    if (isConnectingChain) {
      toast({
        title: t('connection.errors.chain_busy_title', { defaultValue: 'Operation in Progress' }),
        description: t('connection.errors.chain_busy_desc', { defaultValue: 'Another connection chain is in progress. Please wait.' }),
        variant: 'error',
      });
      return;
    }
    
    if (isNodeConnecting(nodeId)) {
      toast({
        title: t('connection.errors.node_connecting_title', { defaultValue: 'Already Connecting' }),
        description: t('connection.errors.node_connecting_desc', { defaultValue: 'This node is already being connected.' }),
        variant: 'error',
      });
      return;
    }
    
    try {
      // 1. 使用线性连接器建立 SSH 连接链
      const connectedNodeIds = await connectNodeWithAncestors(nodeId);
      console.log(`[handleTreeConnect] Connected ${connectedNodeIds.length} nodes`);
      
      // 2. 获取目标节点的连接 ID
      await fetchTree(); // 确保状态同步
      const node = getNode(nodeId);
      if (!node?.runtime.connectionId) {
        throw new Error('Connection ID not found after connect');
      }
      
      // 3. 创建终端会话
      const terminalResponse = await api.createTerminal({
        connectionId: node.runtime.connectionId,
        cols: 80,
        rows: 24,
      });

      // 4. 把 session 添加到 appStore.sessions
      useAppStore.setState((state) => {
        const newSessions = new Map(state.sessions);
        newSessions.set(terminalResponse.sessionId, terminalResponse.session);

        // 更新连接的 terminalIds 和 refCount
        const newConnections = new Map(state.connections);
        const connection = newConnections.get(node.runtime.connectionId!);
        if (connection) {
          newConnections.set(node.runtime.connectionId!, {
            ...connection,
            terminalIds: [terminalResponse.sessionId],
            refCount: 1,
            state: 'active',
          });
        }

        return { sessions: newSessions, connections: newConnections };
      });

      // 5. 关联终端会话到节点
      await api.setTreeNodeTerminal(nodeId, terminalResponse.sessionId);

      // 6. 刷新树和连接池
      await Promise.all([
        fetchTree(),
        refreshConnections(),
      ]);

      // 7. 打开终端 tab
      createTab('terminal', terminalResponse.sessionId);
      
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      console.error('[handleTreeConnect] Failed:', errorMsg);
      
      // 根据错误类型显示不同提示
      if (errorMsg.includes('CHAIN_LOCK_BUSY')) {
        toast({
          title: t('connection.errors.chain_busy_title', { defaultValue: 'Operation in Progress' }),
          description: t('connection.errors.chain_busy_desc', { defaultValue: 'Another connection chain is in progress. Please wait.' }),
          variant: 'error',
        });
      } else if (errorMsg.includes('NODE_LOCK_BUSY')) {
        toast({
          title: t('connection.errors.node_connecting_title', { defaultValue: 'Already Connecting' }),
          description: t('connection.errors.node_connecting_desc', { defaultValue: 'This node is already being connected.' }),
          variant: 'error',
        });
      } else if (errorMsg.includes('CONNECTION_CHAIN_FAILED')) {
        // 解析失败节点信息
        const match = errorMsg.match(/Node ([\w-]+) \(position (\d+)\/(\d+)\) failed: (.+)/);
        if (match) {
          toast({
            title: t('connection.errors.chain_failed_title', { defaultValue: 'Connection Failed' }),
            description: t('connection.errors.chain_failed_desc', { 
              defaultValue: 'Failed at node {{position}}/{{total}}: {{error}}',
              position: match[2],
              total: match[3],
              error: match[4],
            }),
            variant: 'error',
          });
        } else {
          toast({
            title: t('connection.errors.chain_failed_title', { defaultValue: 'Connection Failed' }),
            description: errorMsg,
            variant: 'error',
          });
        }
      } else {
        toast({
          title: t('connection.errors.generic_title', { defaultValue: 'Connection Error' }),
          description: errorMsg,
          variant: 'error',
        });
      }
      
      // 刷新树以显示错误状态
      await fetchTree();
    }
  }, [fetchTree, refreshConnections, createTab, getNode, toast, t]);

  const handleTreeDisconnect = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    const displayName = node?.displayName || `${node?.username}@${node?.host}`;

    // Confirm disconnect via async dialog (window.confirm is unreliable in Tauri WebView)
    if (!await confirm({
      title: t('common.confirm.disconnect_node', { name: displayName }),
      variant: 'danger',
    })) {
      return;
    }

    try {
      // Invoke the session tree store's disconnectNode, which will:
      // 1. Close related tabs
      // 2. Terminate the SSH connection
      // 3. Refresh the tree state
      await disconnectNode(nodeId);

      // Refresh connection pool state
      await refreshConnections();
    } catch (err) {
      console.error('Failed to disconnect tree node:', err);
    }
  }, [getNode, disconnectNode, refreshConnections, confirm]);

  const handleTreeOpenSftp = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    if (!node) return;

    const terminalIds = node.runtime?.terminalIds || [];
    const connectionId = node.runtime?.connectionId || node.sshConnectionId;

    // 如果已有终端会话，用第一个打开 SFTP 标签页
    if (terminalIds.length > 0) {
      const sessionId = terminalIds[0];
      createTab('sftp', sessionId, { nodeId });
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开 SFTP 标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('sftp', terminalId, { nodeId });
      } catch (err) {
        console.error('Failed to create session for SFTP:', err);
      }
    }
  }, [getNode, createTab, createTerminalForNode]);

  // 打开 IDE 模式标签页
  const handleTreeOpenIde = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    if (!node) return;

    const terminalIds = node.runtime?.terminalIds || [];
    const connectionId = node.runtime?.connectionId || node.sshConnectionId;

    // 如果已有终端会话，用第一个打开 IDE 标签页
    if (terminalIds.length > 0) {
      const sessionId = terminalIds[0];
      createTab('ide', sessionId, { nodeId });
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开 IDE 标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('ide', terminalId, { nodeId });
      } catch (err) {
        console.error('Failed to create session for IDE:', err);
      }
    }
  }, [getNode, createTab, createTerminalForNode]);

  // 打开端口转发标签页
  const handleTreeOpenForwards = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    if (!node) return;

    const terminalIds = node.runtime?.terminalIds || [];
    const connectionId = node.runtime?.connectionId || node.sshConnectionId;

    // 如果节点有终端，用第一个
    if (terminalIds.length > 0) {
      const sessionId = terminalIds[0];
      createTab('forwards', sessionId, { nodeId });
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开转发标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('forwards', terminalId, { nodeId });
      } catch (err) {
        console.error('Failed to create session for forwards:', err);
      }
    }
  }, [getNode, createTab, createTerminalForNode]);

  const handleTreeRemove = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    const displayName = node?.displayName || `${node?.username}@${node?.host}`;
    if (await confirm({
      title: t('common.confirm.remove_node', { name: displayName }),
      variant: 'danger',
    })) {
      try {
        await removeNode(nodeId);
      } catch (err) {
        console.error('Failed to remove tree node:', err);
      }
    }
  }, [getNode, removeNode, confirm]);

  const handleTreeSaveAsPreset = useCallback((nodeId: string) => {
    setSavePresetDialog({ open: true, nodeId });
  }, []);

  // 新建终端 (使用统一 store)
  const handleTreeNewTerminal = useCallback(async (nodeId: string) => {
    try {
      const terminalId = await createTerminalForNode(nodeId, 80, 24);
      createTab('terminal', terminalId);
    } catch (err) {
      console.error('Failed to create terminal:', err);
      const errMsg = String(err);
      if (errMsg.includes('CONNECTION_RECONNECTING')) {
        toast({
          title: t('connections.status.reconnecting_title'),
          description: t('connections.status.reconnecting_wait'),
          variant: 'warning',
        });
      }
    }
  }, [createTerminalForNode, createTab, toast, t]);

  // 关闭终端
  const handleTreeCloseTerminal = useCallback(async (nodeId: string, terminalId: string) => {
    try {
      // 关闭对应的 tab
      const tab = tabs.find(t => t.sessionId === terminalId);
      if (tab) {
        closeTab(tab.id);
      }
      await closeTerminalForNode(nodeId, terminalId);
    } catch (err) {
      console.error('Failed to close terminal:', err);
    }
  }, [closeTerminalForNode, tabs, closeTab]);

  // 选择终端 (切换 tab)
  const handleTreeSelectTerminal = useCallback((terminalId: string) => {
    const existingTab = tabs.find(t => t.sessionId === terminalId && t.type === 'terminal');
    if (existingTab) {
      setActiveTab(existingTab.id);
    } else {
      createTab('terminal', terminalId);
    }
  }, [tabs, setActiveTab, createTab]);

  // 重连节点
  const handleTreeReconnect = useCallback(async (nodeId: string) => {
    try {
      // 防御性清理：关闭该节点的所有残留 tabs（正常情况下 useConnectionEvents 已在 link_down 时关闭）
      // 这里再检查一次以防万一有遗漏
      const nodeBeforeReconnect = getNode(nodeId);
      if (nodeBeforeReconnect?.runtime?.terminalIds) {
        const oldTerminalIds = new Set(nodeBeforeReconnect.runtime.terminalIds);
        const tabsToClose = tabs.filter(tab => tab.sessionId && oldTerminalIds.has(tab.sessionId));
        if (tabsToClose.length > 0) {
          console.log(`[Reconnect] Closing ${tabsToClose.length} stale tabs before reconnect`);
          for (const tab of tabsToClose) {
            closeTab(tab.id);
          }
          // 短暂延迟让 React 完成卸载
          await new Promise(r => setTimeout(r, 100));
        }
      }
      
      await connectNode(nodeId);

      // 等待一小段时间让后端完成异步初始化并发出 connection_status_changed 事件
      // 这样新的 connectionId 会被添加到 appStore.connections 中
      await new Promise(resolve => setTimeout(resolve, 500));

      // 连接成功后，获取 connectionId 并等待连接真正稳定
      // connectNode 返回后，后端可能还在做一些异步初始化
      const node = getNode(nodeId);
      const connectionId = node?.runtime?.connectionId || node?.sshConnectionId;

      if (connectionId) {
        // Node-first: 轮询节点状态等待连接稳定（最多 20 秒）
        const deadline = Date.now() + 20000;
        let stable = false;
        while (Date.now() < deadline) {
          const freshNode = getNode(nodeId);
          if (freshNode?.runtime?.status === 'connected') {
            stable = true;
            break;
          }
          await new Promise(r => setTimeout(r, 500));
        }
        if (!stable) {
          const freshNode = getNode(nodeId);
          if (freshNode?.runtime?.status !== 'connected') {
            console.error('Connection not stable after wait');
            toast({
              title: t('connections.status.reconnect_unstable'),
              description: t('connections.status.try_again_later'),
              variant: 'warning',
            });
            return;
          }
        }
      } else {
        console.error('[Reconnect] No connectionId found for node after connectNode');
        toast({
          title: t('connections.status.connection_failed'),
          description: t('connections.status.no_connection_id'),
          variant: 'error',
        });
        return;
      }
      
      // 获取断开前保存的终端数量
      const { disconnectedTerminalCounts } = useSessionTreeStore.getState();
      const terminalCountToRestore = disconnectedTerminalCounts.get(nodeId) || 1;
      
      // 重连成功后，恢复之前数量的终端
      // 如果之前没有记录，默认创建 1 个
      for (let i = 0; i < terminalCountToRestore; i++) {
        // 带重试的终端创建（处理 CONNECTION_RECONNECTING 错误）
        let terminalId: string | null = null;
        let lastErr: unknown = null;
        
        for (let attempt = 0; attempt < 3 && !terminalId; attempt++) {
          try {
            terminalId = await createTerminalForNode(nodeId, 80, 24);
          } catch (termErr) {
            lastErr = termErr;
            const errMsg = String(termErr);
            if (errMsg.includes('CONNECTION_') || errMsg.includes('RECONNECTING') || errMsg.includes('SESSION_NOT_FOUND')) {
              // 连接还在重连中，等待后重试
              console.log(`Terminal ${i + 1} creation blocked by reconnecting, retry ${attempt + 1}/3`);
              await new Promise(r => setTimeout(r, 1000 * (attempt + 1)));
            } else {
              // 其他错误，不重试
              break;
            }
          }
        }
        
        if (terminalId) {
          // 等待 backend WS bridge 完全就绪后再创建 Tab
          // 增加到 500ms 确保 WS bridge 完全就绪
          await new Promise(r => setTimeout(r, 500));
          createTab('terminal', terminalId);
          // 更长的延迟避免同时创建太多终端争用资源
          if (i < terminalCountToRestore - 1) {
            await new Promise(r => setTimeout(r, 500));
          }
        } else {
          console.error(`Failed to create terminal ${i + 1}/${terminalCountToRestore}:`, lastErr);
        }
      }
      
      // 清除保存的终端数量
      useSessionTreeStore.setState((state) => {
        const newCounts = new Map(state.disconnectedTerminalCounts);
        newCounts.delete(nodeId);
        return { disconnectedTerminalCounts: newCounts };
      });
    } catch (err) {
      console.error('Failed to reconnect:', err);
    }
  }, [connectNode, createTerminalForNode, createTab, getNode, toast, t, tabs, closeTab]);

  // 从 Saved Connections 连接 - 使用提取后的共享函数
  const handleConnectSaved = useCallback(async (connectionId: string) => {
    await connectToSaved(connectionId, {
      createTab,
      toast,
      t,
      onError: openConnectionEditor,
    });
  }, [openConnectionEditor, createTab, toast, t]);

  // ═══════════════════════════════════════════════════════════════════════════
  // Data-driven sidebar button definitions
  // ═══════════════════════════════════════════════════════════════════════════

  type SidebarButtonKind = 'section' | 'tab' | 'toggle' | 'action';
  type SidebarButtonDef = {
    kind: SidebarButtonKind;
    key: string;
    icon: React.ComponentType<{ className?: string }>;
    titleKey: string;
    badge?: number;
    badgeColor?: string;
    separator?: boolean;
  };

  // Plugin-registered sidebar panels (inserted between main buttons and bottom group)
  const pluginPanelDefs: SidebarButtonDef[] = Array.from(
    usePluginStore.getState().sidebarPanels.values()
  ).map(panel => ({
    kind: 'section' as const,
    key: `plugin:${panel.pluginId}:${panel.panelId}`,
    icon: resolvePluginIcon(panel.icon),
    titleKey: panel.title,
  }));

  const topButtons: SidebarButtonDef[] = [
    { kind: 'section', key: 'sessions', icon: Link2, titleKey: 'sidebar.panels.sessions', separator: true },
    { kind: 'section', key: 'saved', icon: Database, titleKey: 'sidebar.panels.saved' },
    { kind: 'tab', key: 'session_manager', icon: LayoutList, titleKey: 'sidebar.panels.session_manager' },
    { kind: 'tab', key: 'connection_pool', icon: Terminal, titleKey: 'sidebar.panels.connection_pool', badge: connections.size > 0 ? connections.size : undefined, badgeColor: 'bg-green-500' },
    { kind: 'tab', key: 'connection_monitor', icon: Activity, titleKey: 'sidebar.panels.connection_monitor' },
    { kind: 'tab', key: 'topology', icon: Network, titleKey: 'sidebar.panels.connection_matrix' },
    { kind: 'toggle', key: 'ai', icon: Sparkles, titleKey: 'sidebar.panels.ai' },
    // Plugin-registered sidebar panels
    ...pluginPanelDefs,
  ];

  const bottomButtons: SidebarButtonDef[] = [
    { kind: 'action', key: 'local_terminal', icon: Square, titleKey: 'sidebar.actions.new_local_terminal', badge: localTerminals.size > 0 ? localTerminals.size : undefined, badgeColor: 'bg-blue-500' },
    { kind: 'tab', key: 'file_manager', icon: FolderOpen, titleKey: 'sidebar.panels.files' },
    ...(!platform.isLinux ? [{ kind: 'tab' as const, key: platform.isMac ? 'launcher' as const : 'graphics' as const, icon: Monitor, titleKey: platform.isMac ? 'launcher.tabTitle' : 'graphics.tab_title' }] : []),
    { kind: 'tab', key: 'plugin_manager', icon: Puzzle, titleKey: 'sidebar.panels.plugins' },
    { kind: 'tab', key: 'settings', icon: Settings, titleKey: 'sidebar.tooltips.settings' },
  ];

  const getButtonVariant = (def: SidebarButtonDef): 'secondary' | 'ghost' => {
    if (def.kind === 'section') {
      return sidebarActiveSection === def.key ? 'secondary' : 'ghost';
    }
    if (def.kind === 'tab') {
      return tabs.find(tab => tab.id === activeTabId)?.type === def.key ? 'secondary' : 'ghost';
    }
    if (def.kind === 'toggle' && def.key === 'ai') {
      return !aiSidebarCollapsed ? 'secondary' : 'ghost';
    }
    return 'ghost';
  };

  const handleButtonClick = (def: SidebarButtonDef, collapsed: boolean) => {
    if (def.kind === 'section') {
      setSidebarSection(def.key as Parameters<typeof setSidebarSection>[0]);
      if (collapsed) toggleSidebar();
    } else if (def.kind === 'tab') {
      createTab(def.key as Parameters<typeof createTab>[0]);
    } else if (def.kind === 'toggle' && def.key === 'ai') {
      toggleAiSidebar();
    } else if (def.kind === 'action' && def.key === 'local_terminal') {
      handleNewLocalTerminal();
    }
  };

  const renderSidebarButton = (def: SidebarButtonDef, collapsed: boolean) => {
    const Icon = def.icon;
    const btn = (
      <Button
        variant={getButtonVariant(def)}
        size="icon"
        onClick={() => handleButtonClick(def, collapsed)}
        title={t(def.titleKey)}
        className="rounded-md h-9 w-9"
      >
        <Icon className="h-5 w-5" />
      </Button>
    );

    if (def.badge !== undefined) {
      return (
        <React.Fragment key={def.key}>
          {def.separator && <div className="w-6 h-px bg-theme-border my-1" />}
          <div className="relative">
            {btn}
            <span className={`absolute -top-1 -right-1 ${def.badgeColor ?? 'bg-green-500'} text-[10px] text-white rounded-full min-w-[14px] h-[14px] flex items-center justify-center px-0.5 pointer-events-none`}>
              {def.badge}
            </span>
          </div>
        </React.Fragment>
      );
    }

    return (
      <React.Fragment key={def.key}>
        {def.separator && <div className="w-6 h-px bg-theme-border my-1" />}
        {btn}
      </React.Fragment>
    );
  };

  // Collapsed state: only show activity bar
  if (sidebarCollapsed) {
    return (
      <div className="flex h-full border-r border-theme-border bg-theme-bg-panel flex-row">
        {/* Activity Bar Only (Collapsed) */}
        <div className="flex flex-col items-center py-2 gap-2 w-12 bg-theme-bg shrink-0">
          {/* Expand Button */}
          <Button
            variant="ghost"
            size="icon"
            onClick={toggleSidebar}
            title={t('sidebar.actions.expand')}
            className="rounded-md h-9 w-9"
          >
            <PanelLeft className="h-5 w-5" />
          </Button>

          {/* Top + middle: scrollable when plugins overflow */}
          <div className="flex-1 flex flex-col items-center gap-2 min-h-0 overflow-y-auto overflow-x-hidden scrollbar-none self-stretch">
            {topButtons.map(def => renderSidebarButton(def, true))}
          </div>

          {/* Bottom: fixed */}
          <div className="flex flex-col items-center gap-2 shrink-0">
            <div className="w-6 h-px bg-theme-border" />
            {bottomButtons.map(def => renderSidebarButton(def, true))}
          </div>
        </div>
      </div>
    );
  }

  const sessionList = Array.from(sessions.values());
  void sessionList; // For future use

  return (
    <div
      ref={sidebarRef}
      className="flex h-full border-r border-theme-border bg-theme-bg-panel flex-row relative"
      style={{ width: sidebarWidth }}
    >
      {/* Activity Bar (Vertical Left) */}
      <div className="flex flex-col items-center py-2 gap-2 border-r border-theme-border w-12 bg-theme-bg shrink-0">
        {/* Collapse Button */}
        <Button
          variant="ghost"
          size="icon"
          onClick={toggleSidebar}
          title={t('sidebar.actions.collapse')}
          className="rounded-md h-9 w-9"
        >
          <PanelLeftClose className="h-5 w-5" />
        </Button>

        {/* Top + middle: scrollable when plugins overflow */}
        <div className="flex-1 flex flex-col items-center gap-2 min-h-0 overflow-y-auto overflow-x-hidden scrollbar-none self-stretch">
          {topButtons.map(def => renderSidebarButton(def, false))}
        </div>

        {/* Bottom: fixed */}
        <div className="flex flex-col items-center gap-2 shrink-0">
          <div className="w-6 h-px bg-theme-border" />
          {bottomButtons.map(def => renderSidebarButton(def, false))}
        </div>
      </div>

      {/* Content Area */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <div className="flex-1 overflow-y-auto p-2">
          {sidebarActiveSection === 'sessions' && (
            <div className="space-y-4 flex flex-col h-full">
              <div className="flex items-center justify-between px-2 min-w-0">
                <span className="text-xs font-semibold text-theme-text-muted uppercase tracking-wider truncate">{t('sidebar.panels.sessions')}</span>
                <div className="flex items-center gap-1">
                  {/* 视图模式切换 */}
                  <Button
                    variant={viewMode === 'focus' ? 'secondary' : 'ghost'}
                    size="icon"
                    className="h-6 w-6"
                    onClick={() => setViewMode(viewMode === 'focus' ? 'tree' : 'focus')}
                    title={viewMode === 'focus' ? '切换到树形视图' : '切换到聚焦视图'}
                  >
                    {viewMode === 'focus' ? (
                      <ListChecks className="h-3 w-3" />
                    ) : (
                      <Folder className="h-3 w-3" />
                    )}
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    onClick={() => toggleModal('autoRoute', true)}
                    title="Auto-Route Connection"
                  >
                    <Network className="h-3 w-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    onClick={() => toggleModal('newConnection', true)}
                    title="New Connection"
                  >
                    <Plus className="h-3 w-3" />
                  </Button>
                </div>
              </div>

              {/* 聚焦模式：面包屑 + 聚焦节点列表 */}
              {viewMode === 'focus' ? (
                <div className="flex flex-col flex-1 min-h-0">
                  {/* 面包屑导航 */}
                  <Breadcrumb
                    path={getBreadcrumbPath()}
                    onNavigate={setFocusedNode}
                  />

                  {/* 聚焦节点列表 */}
                  <FocusedNodeList
                    focusedNode={getFocusedNodeId() ? getNode(getFocusedNodeId()!) || null : null}
                    children={getVisibleNodes()}
                    selectedNodeId={selectedNodeId}
                    activeTerminalId={activeTabId ? tabs.find(t => t.id === activeTabId)?.sessionId : null}
                    onSelect={selectNode}
                    onEnter={enterNode}
                    onConnect={handleTreeConnect}
                    onDisconnect={handleTreeDisconnect}
                    onReconnect={handleTreeReconnect}
                    onNewTerminal={handleTreeNewTerminal}
                    onCloseTerminal={handleTreeCloseTerminal}
                    onSelectTerminal={handleTreeSelectTerminal}
                    onOpenSftp={handleTreeOpenSftp}
                    onOpenForwards={handleTreeOpenForwards}
                    onDrillDown={handleTreeDrillDown}
                    onRemove={handleTreeRemove}
                  />
                </div>
              ) : (
                /* 传统树形视图 */
                <SessionTree
                  nodes={treeNodes}
                  selectedNodeId={selectedNodeId}
                  activeTerminalId={activeTabId ? tabs.find(t => t.id === activeTabId)?.sessionId : null}
                  onSelectNode={selectNode}
                  onToggleExpand={toggleExpand}
                  onConnect={handleTreeConnect}
                  onDisconnect={handleTreeDisconnect}
                  onReconnect={handleTreeReconnect}
                  onNewTerminal={handleTreeNewTerminal}
                  onCloseTerminal={handleTreeCloseTerminal}
                  onSelectTerminal={handleTreeSelectTerminal}
                  onOpenSftp={handleTreeOpenSftp}
                  onOpenIde={handleTreeOpenIde}
                  onOpenForwards={handleTreeOpenForwards}
                  onDrillDown={handleTreeDrillDown}
                  onRemove={handleTreeRemove}
                  onSaveAsPreset={handleTreeSaveAsPreset}
                />
              )}
            </div>
          )}

          {/* Saved Connections Section (Slim: quick-connect list + open Session Manager) */}
          {sidebarActiveSection === 'saved' && (
            <div className="flex flex-col h-full space-y-2">
              <div className="flex items-center justify-between px-2 min-w-0">
                <span className="text-xs font-semibold text-theme-text-muted uppercase tracking-wider truncate">
                  {t('sidebar.panels.saved_title')}
                </span>
              </div>

              {/* Quick search */}
              <div className="px-2">
                <div className="relative">
                  <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3 w-3 text-theme-text-muted pointer-events-none" />
                  <input
                    type="text"
                    value={savedSearchQuery}
                    onChange={(e) => setSavedSearchQuery(e.target.value)}
                    placeholder={t('sessionManager.toolbar.search_placeholder')}
                    className="w-full h-7 pl-7 pr-2 text-xs rounded bg-theme-bg border border-theme-border text-theme-text placeholder:text-theme-text-muted focus:outline-none focus:ring-1 focus:ring-theme-accent"
                  />
                </div>
              </div>

              {/* Connections List (simple click-to-connect) */}
              <div className="flex-1 overflow-y-auto space-y-0.5 px-1">
                {(() => {
                  const filtered = savedSearchQuery.trim()
                    ? savedConnections.filter(c => {
                        const q = savedSearchQuery.toLowerCase();
                        return c.name.toLowerCase().includes(q)
                          || c.host.toLowerCase().includes(q)
                          || c.username.toLowerCase().includes(q)
                          || (c.group || '').toLowerCase().includes(q);
                      })
                    : savedConnections;

                  if (filtered.length === 0) {
                    return (
                      <div className="text-xs text-theme-text-muted px-2 py-4 text-center">
                        {savedSearchQuery ? t('sessionManager.table.no_search_results') : t('sidebar.panels.no_saved_connections')}
                      </div>
                    );
                  }

                  return filtered.map(conn => (
                    <div
                      key={conn.id}
                      onClick={() => handleConnectSaved(conn.id)}
                      className="flex items-center gap-2 px-2 py-1.5 text-sm rounded-sm cursor-pointer group text-theme-text hover:bg-theme-bg-hover transition-colors"
                    >
                      {conn.color && (
                        <div
                          className="w-0.5 h-6 rounded-full shrink-0"
                          style={{ backgroundColor: conn.color }}
                        />
                      )}
                      <Server className="h-3 w-3 text-theme-text-muted shrink-0" />
                      <div className="flex-1 min-w-0">
                        <div className="truncate font-medium text-xs">{conn.name}</div>
                        <div className="text-[10px] text-theme-text-muted truncate">
                          {conn.username}@{conn.host}:{conn.port}
                        </div>
                      </div>
                      <ChevronRight className="h-3 w-3 text-theme-text-muted opacity-0 group-hover:opacity-100 shrink-0" />
                    </div>
                  ));
                })()}
              </div>

              {/* Open Session Manager button */}
              <div className="shrink-0 border-t border-theme-border px-2 py-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full gap-1.5 text-xs min-w-0"
                  onClick={() => createTab('session_manager')}
                >
                  <LayoutList className="h-3 w-3 shrink-0" />
                  <span className="truncate">{t('sidebar.panels.open_session_manager')}</span>
                </Button>
              </div>
            </div>
          )}


          {/* Plugin Sidebar Panels */}
          {typeof sidebarActiveSection === 'string' && sidebarActiveSection.startsWith('plugin:') && (
            <div className="space-y-4 flex flex-col h-full">
              <PluginSidebarRenderer panelKey={sidebarActiveSection.replace('plugin:', '')} />
            </div>
          )}

        </div>
      </div>

      {/* Edit Connection Modal */}
      <EditConnectionModal
        open={modals.editConnection}
        onOpenChange={(open) => toggleModal('editConnection', open)}
        connection={editingConnection}
        onConnect={() => {
          loadSavedConnections();
        }}
      />

      {/* DrillDown Dialog */}
      <DrillDownDialog
        open={drillDownDialog.open}
        onOpenChange={(open) => setDrillDownDialog(prev => ({ ...prev, open }))}
        parentNodeId={drillDownDialog.parentId}
        parentHost={drillDownDialog.parentHost}
        onSuccess={async () => {
          await fetchTree();
        }}
      />

      {/* Save As Preset Dialog */}
      <SavePathAsPresetDialog
        isOpen={savePresetDialog.open}
        onClose={() => setSavePresetDialog({ open: false, nodeId: '' })}
        targetNodeId={savePresetDialog.nodeId}
        nodes={treeNodes}
        onSaved={() => {
          loadSavedConnections();
        }}
      />

      {/* Add Root Node Dialog */}
      <AddRootNodeDialog
        open={addRootNodeOpen}
        onOpenChange={setAddRootNodeOpen}
        onSuccess={async () => {
          await fetchTree();
        }}
      />

      {/* Resize Handle */}
      <div
        className={cn(
          "absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-theme-accent/50 transition-colors z-10",
          isResizing && "bg-theme-accent"
        )}
        onMouseDown={handleMouseDown}
      />
      {ConfirmDialog}
    </div>
  );
};
