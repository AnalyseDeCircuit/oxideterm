// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * useNodeState — 订阅单个节点的实时状态 (Oxide-Next Phase 3)
 *
 * 设计目标：
 *   - node:state 事件统一进入全局缓存（refcount bridge）
 *   - hook 只负责读取 store，不再自行直连 Tauri 事件
 *   - 首个相关视图挂载时才激活 bridge，避免应用启动即常驻监听
 *
 * 参考: docs/reference/OXIDE_NEXT_ARCHITECTURE.md §4.2
 */

import { useEffect } from 'react';
import { useNodeStateStore } from '../store/nodeStateStore';
import { retainNodeStateBridge } from '../store/nodeStateStore';
import type { NodeState } from '../types';

/** useNodeState 返回值 */
export type UseNodeStateResult = {
  /** 节点完整状态 */
  state: NodeState;
  /** 当前 generation（单调递增） */
  generation: number;
  /** 初始快照是否已加载 */
  ready: boolean;
};

/** 默认初始状态 */
const INITIAL_STATE: NodeState = {
  readiness: 'disconnected',
  sftpReady: false,
};

/**
 * 订阅指定节点的实时状态。
 *
 * @param nodeId 节点 ID（来自 SessionTree）
 * @returns 节点状态、generation、加载就绪标志
 *
 * @example
 * ```tsx
 * function TerminalView({ nodeId }: { nodeId: string }) {
 *   const { state, ready } = useNodeState(nodeId);
 *   if (!ready) return <Loading />;
 *   if (state.readiness === 'error') return <ErrorView error={state.error} />;
 *   // ...
 * }
 * ```
 */
export function useNodeState(nodeId: string | undefined): UseNodeStateResult {
  const state = useNodeStateStore((store) =>
    nodeId ? store.getEntry(nodeId).snapshot.state : INITIAL_STATE,
  );
  const generation = useNodeStateStore((store) =>
    nodeId ? store.getEntry(nodeId).snapshot.generation : 0,
  );
  const ready = useNodeStateStore((store) =>
    nodeId ? store.getEntry(nodeId).ready : false,
  );

  useEffect(() => {
    const release = retainNodeStateBridge();
    return release;
  }, []);

  return { state, generation, ready };
}
