import React, { useEffect, useState, useCallback, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Terminal,
  Folder,
  FolderOpen,
  Settings,
  Plus,
  ChevronRight,
  ChevronDown,
  Server,
  Trash2,
  ListChecks,
  Check,
  Download,
  Upload,
  Link2,
  Activity,
  Network,
  Database,
  Sparkles,
  Square,
  PanelLeftClose,
  PanelLeft,
  HeartPulse,
} from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { useToast } from '../../hooks/useToast';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';
import { Checkbox } from '../ui/checkbox';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { EditConnectionModal } from '../modals/EditConnectionModal';
import { OxideExportModal } from '../modals/OxideExportModal';
import { OxideImportModal } from '../modals/OxideImportModal';
import { SessionTree } from '../sessions/SessionTree';
import { Breadcrumb } from '../sessions/Breadcrumb';
import { FocusedNodeList } from '../sessions/FocusedNodeList';
import { DrillDownDialog } from '../modals/DrillDownDialog';
import { SavePathAsPresetDialog } from '../modals/SavePathAsPresetDialog';
import { AddRootNodeDialog } from '../modals/AddRootNodeDialog';
import { api } from '../../lib/api';
import { waitForConnectionActive, isConnectionGuardError } from '../../lib/connectionGuard';
import type { UnifiedFlatNode } from '../../types';
import { SystemHealthPanel } from './SystemHealthPanel';

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

  const {
    setSidebarSection,
    sessions,
    connections,
    toggleModal,
    createTab,
    closeTab,
    tabs,
    activeTabId,
    setActiveTab,
    savedConnections,
    groups,
    selectedGroup,
    loadSavedConnections,
    loadGroups,
    setSelectedGroup,
    modals,
    editingConnection,
    refreshConnections,
    openConnectionEditor,
  } = useAppStore();

  // SessionTree store
  const {
    nodes: treeNodes,
    selectedNodeId,
    getFocusedNodeId,
    fetchTree,
    selectNode,
    toggleExpand,
    removeNode,
    getNode,
    createTerminalForNode,
    closeTerminalForNode,
    connectNode,
    disconnectNode,
    addRootNode,
    setFocusedNode,
    getBreadcrumbPath,
    getVisibleNodes,
    enterNode,
  } = useSessionTreeStore();

  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set(['ungrouped']));
  const [isManageMode, setIsManageMode] = useState(false);
  const [selectedConnections, setSelectedConnections] = useState<Set<string>>(new Set());
  const [showExportModal, setShowExportModal] = useState(false);
  const [showImportModal, setShowImportModal] = useState(false);

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
  const { createTerminal: createLocalTerminal, terminals: localTerminals } = useLocalTerminalStore();

  // Toast hook (需要在所有使用 toast 的 useCallback 之前声明)
  const { toast } = useToast();

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

    // Confirm disconnect
    if (!window.confirm(t('common.confirm.disconnect_node', { name: displayName }))) {
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
  }, [getNode, disconnectNode, refreshConnections]);

  const handleTreeOpenSftp = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    if (!node) return;

    const terminalIds = node.runtime?.terminalIds || [];
    const connectionId = node.runtime?.connectionId || node.sshConnectionId;

    // 如果已有终端会话，用第一个打开 SFTP 标签页
    if (terminalIds.length > 0) {
      const sessionId = terminalIds[0];
      createTab('sftp', sessionId);
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开 SFTP 标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('sftp', terminalId);
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
      createTab('ide', sessionId);
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开 IDE 标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('ide', terminalId);
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
      createTab('forwards', sessionId);
      return;
    }

    // 如果节点已连接但没有终端会话，先创建终端会话再打开转发标签页
    if (connectionId && (node.runtime.status === 'connected' || node.runtime.status === 'active')) {
      try {
        const terminalId = await createTerminalForNode(nodeId, 80, 24);
        createTab('forwards', terminalId);
      } catch (err) {
        console.error('Failed to create session for forwards:', err);
      }
    }
  }, [getNode, createTab, createTerminalForNode]);

  const handleTreeRemove = useCallback(async (nodeId: string) => {
    const node = getNode(nodeId);
    const displayName = node?.displayName || `${node?.username}@${node?.host}`;
    if (window.confirm(t('common.confirm.remove_node', { name: displayName }))) {
      try {
        await removeNode(nodeId);
      } catch (err) {
        console.error('Failed to remove tree node:', err);
      }
    }
  }, [getNode, removeNode]);

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
        try {
          // 等待连接状态变为 active（最多 20 秒）
          await waitForConnectionActive(connectionId, 20000);
        } catch (waitErr) {
          // 如果等待超时但节点状态显示已连接，仍然继续尝试创建终端
          const freshNode = getNode(nodeId);
          if (freshNode?.runtime?.status !== 'connected') {
            console.error('Connection not stable after wait:', waitErr);
            toast({
              title: t('connections.status.reconnect_unstable'),
              description: t('connections.status.try_again_later'),
              variant: 'warning',
            });
            return;
          }
          console.warn('Connection wait failed but node shows connected, continuing:', waitErr);
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
            if (isConnectionGuardError(termErr)) {
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

  // 从 Saved Connections 连接 - 在树中创建根节点
  const handleConnectSaved = useCallback(async (connectionId: string) => {
    try {
      // 获取保存连接的完整信息
      const savedConn = await api.getSavedConnectionForConnect(connectionId);

      // 映射 auth_type（带 default_key → key fallback）
      const mapAuthType = (authType: string): 'password' | 'key' | 'agent' | undefined => {
        if (authType === 'agent') return 'agent';
        if (authType === 'key') return 'key';
        if (authType === 'password') return 'password';
        return undefined; // default_key
      };

      // 映射 auth_type（用于 proxy_chain hops，无 default_key）
      const mapPresetAuthType = (authType: string): 'password' | 'key' | 'agent' => {
        if (authType === 'agent') return 'agent';
        if (authType === 'key') return 'key';
        if (authType === 'password') return 'password';
        return 'key'; // default_key fallback to key
      };

      // ========== Phase 3.4: Proxy Chain 支持 ==========
      // 使用 expandManualPreset + connectNodeWithAncestors 实现前端驱动的线性连接
      if (savedConn.proxy_chain && savedConn.proxy_chain.length > 0) {
        const { expandManualPreset, connectNodeWithAncestors, createTerminalForNode } = useSessionTreeStore.getState();

        // 构建预设链请求
        const hops = savedConn.proxy_chain.map((hop: { host: string; port: number; username: string; auth_type: string; password?: string; key_path?: string; passphrase?: string }) => ({
          host: hop.host,
          port: hop.port,
          username: hop.username,
          authType: mapPresetAuthType(hop.auth_type),
          password: hop.password,
          keyPath: hop.key_path,
          passphrase: hop.passphrase,
        }));

        const target = {
          host: savedConn.host,
          port: savedConn.port,
          username: savedConn.username,
          authType: mapPresetAuthType(savedConn.auth_type),
          password: savedConn.password,
          keyPath: savedConn.key_path,
          passphrase: savedConn.passphrase,
        };

        const request = {
          savedConnectionId: connectionId,
          hops,
          target,
        };

        // Step 1: 展开预设链为树节点（不建立连接）
        const expandResult = await expandManualPreset(request);
        
        // Step 2: 使用线性连接器连接整条链路
        await connectNodeWithAncestors(expandResult.targetNodeId);
        
        // Step 3: 为目标节点创建终端并打开标签页
        const terminalId = await createTerminalForNode(expandResult.targetNodeId);
        createTab('terminal', terminalId);

        // 显示成功提示
        toast({
          title: t('connections.toast.proxy_chain_established'),
          description: t('connections.toast.proxy_chain_desc', { depth: expandResult.chainDepth }),
          variant: 'success',
        });

        // 标记连接已使用
        await api.markConnectionUsed(connectionId);
        return;
      }

      // ========== 直连（无 proxy_chain）==========
      // 检查是否已有相同主机的根节点
      const { nodes } = useSessionTreeStore.getState();
      const existingNode = nodes.find((n: UnifiedFlatNode) =>
        n.depth === 0 &&
        n.host === savedConn.host &&
        n.port === savedConn.port &&
        n.username === savedConn.username
      );

      let nodeId: string;

      if (existingNode) {
        // 已存在相同节点 - 直接使用
        nodeId = existingNode.id;
        useSessionTreeStore.setState({ selectedNodeId: nodeId });

        // 如果节点未连接，尝试连接（使用线性连接器）
        if (existingNode.runtime.status === 'idle' || existingNode.runtime.status === 'error') {
          const { connectNodeWithAncestors } = useSessionTreeStore.getState();
          await connectNodeWithAncestors(nodeId);
        }
      } else {
        // 创建新节点
        nodeId = await addRootNode({
          host: savedConn.host,
          port: savedConn.port,
          username: savedConn.username,
          authType: mapAuthType(savedConn.auth_type),
          password: savedConn.password,
          keyPath: savedConn.key_path,
          passphrase: savedConn.passphrase,
          displayName: savedConn.name,
        });

        // 自动连接新创建的节点（使用线性连接器）
        const { connectNodeWithAncestors } = useSessionTreeStore.getState();
        await connectNodeWithAncestors(nodeId);
      }

      // 标记连接已使用
      await api.markConnectionUsed(connectionId);
    } catch (error) {
      console.error('Failed to connect to saved connection:', error);
      // 只有真正的连接错误才打开编辑器，不包括锁错误
      const errorMsg = String(error);
      if (!errorMsg.includes('already connecting') && 
          !errorMsg.includes('already connected') &&
          !errorMsg.includes('CHAIN_LOCK_BUSY') &&
          !errorMsg.includes('NODE_LOCK_BUSY')) {
        openConnectionEditor(connectionId);
      }
    }
  }, [addRootNode, openConnectionEditor, createTab, toast, t]);

  const toggleGroup = (groupName: string) => {
    setExpandedGroups(prev => {
      const next = new Set(prev);
      if (next.has(groupName)) {
        next.delete(groupName);
      } else {
        next.add(groupName);
      }
      return next;
    });
  };

  const toggleConnectionSelection = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedConnections(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleBatchDelete = async () => {
    if (selectedConnections.size === 0) return;

    const count = selectedConnections.size;
    const confirmed = window.confirm(t('common.confirm.delete_batch', { count }));

    if (!confirmed) {
      return; // User cancelled, do nothing
    }

    try {
      // Delete all selected connections
      await Promise.all(
        Array.from(selectedConnections).map(async (id) => {
          try {
            await api.deleteConnection(id);
            console.log(`Successfully deleted connection: ${id}`);
          } catch (err) {
            console.error(`Failed to delete connection ${id}:`, err);
            throw err;
          }
        })
      );

      // Success: Clear selection and refresh list
      setSelectedConnections(new Set());
      await loadSavedConnections();
      console.log(`Successfully deleted ${count} connection(s)`);

    } catch (error: unknown) {
      console.error('Failed to delete connections:', error);
      const message = error instanceof Error ? error.message : String(error);
      alert(t('common.errors.delete_failed', { message }));
      // Refresh list anyway to show which ones were deleted
      await loadSavedConnections();
    }
  };

  const toggleManageMode = () => {
    setIsManageMode(prev => !prev);
    setSelectedConnections(new Set());
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

          <div className="w-6 h-px bg-theme-border my-1" />

          <Button
            variant={sidebarActiveSection === 'sessions' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => { setSidebarSection('sessions'); toggleSidebar(); }}
            title={t('sidebar.panels.sessions')}
            className="rounded-md h-9 w-9"
          >
            <Link2 className="h-5 w-5" />
          </Button>
          <Button
            variant={sidebarActiveSection === 'saved' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => { setSidebarSection('saved'); toggleSidebar(); }}
            title={t('sidebar.panels.saved')}
            className="rounded-md h-9 w-9"
          >
            <Database className="h-5 w-5" />
          </Button>

          {/* SSH Connection Pool (Tab) */}
          <div className="relative">
            <Button
              variant={tabs.find(t => t.id === activeTabId)?.type === 'connection_pool' ? 'secondary' : 'ghost'}
              size="icon"
              onClick={() => createTab('connection_pool')}
              title={t('sidebar.panels.connection_pool')}
              className="rounded-md h-9 w-9"
            >
              <Terminal className="h-5 w-5" />
            </Button>
            {connections.size > 0 && (
              <span className="absolute -top-1 -right-1 bg-green-500 text-[10px] text-white rounded-full min-w-[14px] h-[14px] flex items-center justify-center px-0.5 pointer-events-none">
                {connections.size}
              </span>
            )}
          </div>

          {/* Connection Monitor (Full Tab) */}
          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'connection_monitor' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => createTab('connection_monitor')}
            title={t('sidebar.panels.connection_monitor')}
            className="rounded-md h-9 w-9"
          >
            <Activity className="h-5 w-5" />
          </Button>

          {/* Topology Button */}
          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'topology' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => createTab('topology')}
            title={t('sidebar.panels.connection_matrix')}
            className="rounded-md h-9 w-9"
          >
            <Network className="h-5 w-5" />
          </Button>

          {/* System Health */}
          <Button
            variant={sidebarActiveSection === 'system_health' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => { setSidebarSection('system_health'); toggleSidebar(); }}
            title={t('sidebar.panels.system_health')}
            className="rounded-md h-9 w-9"
          >
            <HeartPulse className="h-5 w-5" />
          </Button>

          {/* AI Sidebar Toggle */}
          <Button
            variant={!aiSidebarCollapsed ? 'secondary' : 'ghost'}
            size="icon"
            onClick={toggleAiSidebar}
            title={t('sidebar.panels.ai')}
            className="rounded-md h-9 w-9"
          >
            <Sparkles className="h-5 w-5" />
          </Button>

          <div className="flex-1" />

          {/* Local Terminal */}
          <div className="relative">
            <Button
              variant="ghost"
              size="icon"
              onClick={handleNewLocalTerminal}
              title={t('sidebar.actions.new_local_terminal')}
              className="rounded-md h-9 w-9"
            >
              <Square className="h-5 w-5" />
            </Button>
            {localTerminals.size > 0 && (
              <span className="absolute -top-1 -right-1 bg-blue-500 text-[10px] text-white rounded-full min-w-[14px] h-[14px] flex items-center justify-center px-0.5 pointer-events-none">
                {localTerminals.size}
              </span>
            )}
          </div>

          {/* File Manager */}
          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'file_manager' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => createTab('file_manager')}
            title={t('sidebar.panels.files')}
            className="rounded-md h-9 w-9"
          >
            <FolderOpen className="h-5 w-5" />
          </Button>

          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'settings' ? 'secondary' : 'ghost'}
            size="icon"
            className="rounded-md h-9 w-9"
            onClick={() => createTab('settings')}
            title={t('sidebar.tooltips.settings')}
          >
            <Settings className="h-5 w-5" />
          </Button>
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

        <div className="w-6 h-px bg-theme-border my-1" />

        <Button
          variant={sidebarActiveSection === 'sessions' ? 'secondary' : 'ghost'}
          size="icon"
          onClick={() => setSidebarSection('sessions')}
          title={t('sidebar.panels.sessions')}
          className="rounded-md h-9 w-9"
        >
          <Link2 className="h-5 w-5" />
        </Button>
        <Button
          variant={sidebarActiveSection === 'saved' ? 'secondary' : 'ghost'}
          size="icon"
          onClick={() => setSidebarSection('saved')}
          title={t('sidebar.panels.saved')}
          className="rounded-md h-9 w-9"
        >
          <Database className="h-5 w-5" />
        </Button>

        {/* SSH Connection Pool (Tab) */}
        <div className="relative">
          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'connection_pool' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => createTab('connection_pool')}
            title={t('sidebar.panels.connection_pool')}
            className="rounded-md h-9 w-9"
          >
            <Terminal className="h-5 w-5" />
          </Button>
          {connections.size > 0 && (
            <span className="absolute -top-1 -right-1 bg-green-500 text-[10px] text-white rounded-full min-w-[14px] h-[14px] flex items-center justify-center px-0.5 pointer-events-none">
              {connections.size}
            </span>
          )}
        </div>

        {/* Connection Monitor (Full Tab) */}
        <Button
          variant={tabs.find(t => t.id === activeTabId)?.type === 'connection_monitor' ? 'secondary' : 'ghost'}
          size="icon"
          onClick={() => createTab('connection_monitor')}
          title={t('sidebar.panels.connection_monitor')}
          className="rounded-md h-9 w-9"
        >
          <Activity className="h-5 w-5" />
        </Button>

        {/* Topology Button */}
        <div className="flex justify-center w-full">
          <Button
            variant={tabs.find(t => t.id === activeTabId)?.type === 'topology' ? 'secondary' : 'ghost'}
            size="icon"
            onClick={() => createTab('topology')}
            title={t('sidebar.panels.connection_matrix')}
            className="rounded-md h-9 w-9"
          >
            <Network className="h-5 w-5" />
          </Button>
        </div>

        {/* System Health */}
        <Button
          variant={sidebarActiveSection === 'system_health' ? 'secondary' : 'ghost'}
          size="icon"
          onClick={() => setSidebarSection('system_health')}
          title={t('sidebar.panels.system_health')}
          className="rounded-md h-9 w-9"
        >
          <HeartPulse className="h-5 w-5" />
        </Button>

        {/* AI Sidebar Toggle */}
        <Button
          variant={!aiSidebarCollapsed ? 'secondary' : 'ghost'}
          size="icon"
          onClick={toggleAiSidebar}
          title={t('sidebar.panels.ai')}
          className="rounded-md h-9 w-9"
        >
          <Sparkles className="h-5 w-5" />
        </Button>

        {/* Local Terminal */}
        <div className="flex-1" />
        <div className="relative">
          <Button
            variant="ghost"
            size="icon"
            onClick={handleNewLocalTerminal}
            title={t('sidebar.actions.new_local_terminal')}
            className="rounded-md h-9 w-9"
          >
            <Square className="h-5 w-5" />
          </Button>
          {localTerminals.size > 0 && (
            <span className="absolute -top-1 -right-1 bg-blue-500 text-[10px] text-white rounded-full min-w-[14px] h-[14px] flex items-center justify-center px-0.5 pointer-events-none">
              {localTerminals.size}
            </span>
          )}
        </div>

        {/* File Manager */}
        <Button
          variant={tabs.find(t => t.id === activeTabId)?.type === 'file_manager' ? 'secondary' : 'ghost'}
          size="icon"
          onClick={() => createTab('file_manager')}
          title={t('sidebar.panels.files')}
          className="rounded-md h-9 w-9"
        >
          <FolderOpen className="h-5 w-5" />
        </Button>

        <Button
          variant={tabs.find(t => t.id === activeTabId)?.type === 'settings' ? 'secondary' : 'ghost'}
          size="icon"
          className="rounded-md h-9 w-9"
          onClick={() => createTab('settings')}
          title={t('sidebar.tooltips.settings')}
        >
          <Settings className="h-5 w-5" />
        </Button>
      </div>

      {/* Content Area */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <div className="flex-1 overflow-y-auto p-2">
          {sidebarActiveSection === 'sessions' && (
            <div className="space-y-4 flex flex-col h-full">
              <div className="flex items-center justify-between px-2">
                <span className="text-xs font-semibold text-theme-text-muted uppercase tracking-wider">{t('sidebar.panels.sessions')}</span>
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

          {/* Saved Connections Section */}
          {sidebarActiveSection === 'saved' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between px-2">
                <span className="text-xs font-semibold text-theme-text-muted uppercase tracking-wider">
                  {t('sidebar.panels.saved_title')}
                </span>
                <div className="flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                    onClick={() => setShowImportModal(true)}
                    title={t('sidebar.panels.import_tooltip')}
                  >
                    <Download className="h-3 w-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover"
                    onClick={() => setShowExportModal(true)}
                    title={t('sidebar.panels.export_tooltip')}
                  >
                    <Upload className="h-3 w-3" />
                  </Button>
                  {isManageMode && selectedConnections.size > 0 && (
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 text-red-500 hover:text-red-400 hover:bg-theme-bg-hover"
                      onClick={handleBatchDelete}
                      title="Delete Selected"
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  )}
                  <Button
                    variant={isManageMode ? "secondary" : "ghost"}
                    size="icon"
                    className={cn("h-6 w-6 text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover", isManageMode && "text-theme-accent bg-theme-bg-hover")}
                    onClick={toggleManageMode}
                    title={isManageMode ? "Done" : "Manage Connections"}
                  >
                    {isManageMode ? <Check className="h-3 w-3" /> : <ListChecks className="h-3 w-3" />}
                  </Button>
                </div>
              </div>

              {/* Group Filter */}
              {groups.length > 0 && (
                <div className="px-2">
                  <Select
                    value={selectedGroup || 'all'}
                    onValueChange={(value) => setSelectedGroup(value === 'all' ? null : value)}
                  >
                    <SelectTrigger className="w-full h-7 text-xs bg-theme-bg-panel border-theme-border text-theme-text hover:bg-theme-bg-hover focus:ring-1 focus:ring-theme-accent">
                      <SelectValue placeholder="All Groups" />
                    </SelectTrigger>
                    <SelectContent className="bg-theme-bg-panel border-theme-border text-theme-text">
                      <SelectItem value="all" className="text-xs">All Groups</SelectItem>
                      {groups.map(group => (
                        <SelectItem key={group} value={group} className="text-xs">{group}</SelectItem>
                      ))}
                      <SelectItem value="ungrouped" className="text-xs">Ungrouped</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              )}

              {/* Connections List */}
              <div className="space-y-1">
                {(() => {
                  const filteredConnections = selectedGroup !== null
                    ? savedConnections.filter(c => c.group === selectedGroup)
                    : savedConnections;

                  // Group connections
                  const grouped = filteredConnections.reduce((acc, conn) => {
                    const groupName = conn.group || 'ungrouped';
                    if (!acc[groupName]) acc[groupName] = [];
                    acc[groupName].push(conn);
                    return acc;
                  }, {} as Record<string, typeof savedConnections>);

                  if (Object.keys(grouped).length === 0) {
                    return (
                      <div className="text-sm text-theme-text-muted px-2 py-4 text-center">
                        {t('sidebar.panels.no_saved_connections')}
                      </div>
                    );
                  }

                  return Object.entries(grouped).map(([groupName, conns]) => (
                    <div key={groupName} className="space-y-1">
                      {/* Group Header */}
                      {Object.keys(grouped).length > 1 && (
                        <div
                          onClick={() => toggleGroup(groupName)}
                          className="flex items-center gap-1 px-2 py-1 text-xs text-theme-text-muted hover:bg-theme-bg-hover rounded-sm cursor-pointer select-none"
                        >
                          {expandedGroups.has(groupName) ? (
                            <ChevronDown className="h-3 w-3" />
                          ) : (
                            <ChevronRight className="h-3 w-3" />
                          )}
                          <span className="font-medium">{groupName}</span>
                          <span className="text-theme-text-muted">({conns.length})</span>
                        </div>
                      )}

                      {/* Group Connections */}
                      {(Object.keys(grouped).length === 1 || expandedGroups.has(groupName)) && conns.map(conn => (
                        <div
                          key={conn.id}
                          onClick={isManageMode ? (e) => toggleConnectionSelection(conn.id, e) : () => handleConnectSaved(conn.id)}
                          className={cn(
                            "flex items-center gap-2 px-2 py-1.5 text-sm rounded-sm cursor-pointer group ml-4 transition-colors",
                            selectedConnections.has(conn.id)
                              ? "bg-theme-accent/20 text-theme-accent hover:bg-theme-accent/30"
                              : "text-theme-text hover:bg-theme-bg-hover"
                          )}
                        >
                          {isManageMode ? (
                            <div className="flex items-center justify-center w-3 h-3">
                              <Checkbox
                                checked={selectedConnections.has(conn.id)}
                                onCheckedChange={() => { }} // Handled by parent click
                                className="h-3 w-3 border-theme-border data-[state=checked]:bg-theme-accent data-[state=checked]:border-theme-accent"
                              />
                            </div>
                          ) : (
                            <Server className="h-3 w-3 text-theme-text-muted" />
                          )}

                          <div className="flex-1 truncate">
                            <div className="truncate font-medium">{conn.name}</div>
                            <div className="text-xs text-theme-text-muted truncate">
                              {conn.username}@{conn.host}:{conn.port}
                            </div>
                          </div>
                          {!isManageMode && (
                            <ChevronRight className="h-3 w-3 text-theme-text-muted opacity-0 group-hover:opacity-100" />
                          )}
                        </div>
                      ))}
                    </div>
                  ));
                })()}
              </div>
            </div>
          )}

          {/* System Health Panel */}
          {sidebarActiveSection === 'system_health' && (
            <div className="space-y-4 flex flex-col h-full">
              <div className="flex items-center justify-between px-2">
                <span className="text-xs font-semibold text-theme-text-muted uppercase tracking-wider">
                  {t('sidebar.panels.system_health')}
                </span>
              </div>
              <SystemHealthPanel />
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

      <OxideExportModal
        isOpen={showExportModal}
        onClose={() => setShowExportModal(false)}
      />

      <OxideImportModal
        isOpen={showImportModal}
        onClose={() => setShowImportModal(false)}
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
    </div>
  );
};
