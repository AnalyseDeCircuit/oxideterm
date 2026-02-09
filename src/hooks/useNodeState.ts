/**
 * useNodeState — 订阅单个节点的实时状态 (Oxide-Next Phase 3)
 *
 * 设计目标：
 *   - 通过 "node:state" Tauri 事件实时接收后端状态推送
 *   - 初始快照通过 node_get_state IPC 获取
 *   - generation 单调递增保证: 丢弃 generation <= 已见最大值的事件（乱序保护）
 *   - 组件卸载时自动清理事件监听
 *
 * 参考: docs/OXIDE_NEXT_ARCHITECTURE.md §4.2
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { nodeGetState } from '../lib/api';
import type { NodeState, NodeStateEvent, NodeReadiness } from '../types';

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
  const [state, setState] = useState<NodeState>(INITIAL_STATE);
  const [generation, setGeneration] = useState(0);
  const [ready, setReady] = useState(false);

  // 使用 ref 记录最大 generation，避免丢弃旧事件时依赖 state
  const maxGenRef = useRef(0);

  // 应用状态更新（仅当 generation 大于已见最大值时）
  const applyUpdate = useCallback(
    (newState: Partial<NodeState>, gen: number) => {
      if (gen <= maxGenRef.current) {
        // 乱序事件，丢弃
        return;
      }
      maxGenRef.current = gen;
      setGeneration(gen);
      setState((prev) => ({ ...prev, ...newState }));
    },
    [],
  );

  useEffect(() => {
    // 无 nodeId 时重置
    if (!nodeId) {
      setState(INITIAL_STATE);
      setGeneration(0);
      setReady(false);
      maxGenRef.current = 0;
      return;
    }

    let mounted = true;
    // 重新订阅新 nodeId 时重置
    maxGenRef.current = 0;
    setReady(false);

    // ---------- 事件监听 ----------
    const setupListener = async () => {
      const unlisten = await listen<NodeStateEvent>('node:state', (event) => {
        if (!mounted) return;
        const payload = event.payload;

        // 仅处理属于当前 nodeId 的事件
        if (payload.nodeId !== nodeId) return;

        switch (payload.type) {
          case 'connectionStateChanged': {
            const partial: Partial<NodeState> = {
              readiness: payload.state as NodeReadiness,
            };
            // 若有 reason 且状态为 error，填充 error 字段
            if (payload.state === 'error' && payload.reason) {
              partial.error = payload.reason;
            } else {
              partial.error = undefined;
            }
            applyUpdate(partial, payload.generation);
            break;
          }
          case 'sftpReady': {
            applyUpdate(
              { sftpReady: payload.ready, sftpCwd: payload.cwd },
              payload.generation,
            );
            break;
          }
          case 'terminalEndpointChanged': {
            applyUpdate(
              {
                wsEndpoint: {
                  wsPort: payload.wsPort,
                  wsToken: payload.wsToken,
                  sessionId: '', // 终端 sessionId 通过快照获取
                },
              },
              payload.generation,
            );
            break;
          }
        }
      });

      return unlisten;
    };

    let unlistenFn: (() => void) | undefined;

    // ---------- 初始快照 + 事件监听并发启动 ----------
    const init = async () => {
      // 先启动事件监听（避免丢失初始快照后的首个事件）
      unlistenFn = await setupListener();

      // 获取初始快照
      try {
        const snapshot = await nodeGetState(nodeId);
        if (!mounted) return;

        // 快照的 generation 可能比已收到的事件更旧，
        // 所以只在快照 generation >= maxGenRef 时应用
        if (snapshot.generation >= maxGenRef.current) {
          maxGenRef.current = snapshot.generation;
          setGeneration(snapshot.generation);
          setState(snapshot.state);
        }
        setReady(true);
      } catch (err) {
        if (!mounted) return;
        // 节点不存在或尚未注册，使用默认状态
        console.warn(`[useNodeState] Failed to get initial state for ${nodeId}:`, err);
        setState(INITIAL_STATE);
        setReady(true);
      }
    };

    init();

    return () => {
      mounted = false;
      unlistenFn?.();
    };
  }, [nodeId, applyUpdate]);

  return { state, generation, ready };
}
