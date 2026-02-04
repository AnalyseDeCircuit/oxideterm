/**
 * 拓扑解析器：将物理连接事件映射到树节点
 * 
 * 职责：
 * 1. connectionId ↔ nodeId 双向映射
 * 2. 处理事件的级联传播
 * 3. 支持批量操作
 * 
 * 生命周期：
 * - connectNode 成功后调用 register()
 * - disconnectNode/removeNode 时调用 unregister()
 * - 应用退出时调用 clear()
 */
class TopologyResolver {
  private connectionToNode: Map<string, string> = new Map();
  private nodeToConnection: Map<string, string> = new Map();

  /**
   * 注册连接与节点的映射
   */
  register(connectionId: string, nodeId: string): void {
    this.connectionToNode.set(connectionId, nodeId);
    this.nodeToConnection.set(nodeId, connectionId);
  }

  /**
   * 清理节点映射（节点断开或删除时）
   */
  unregister(nodeId: string): void {
    const connectionId = this.nodeToConnection.get(nodeId);
    if (connectionId) {
      this.connectionToNode.delete(connectionId);
    }
    this.nodeToConnection.delete(nodeId);
  }

  /**
   * 通过 connectionId 获取 nodeId
   */
  getNodeId(connectionId: string): string | undefined {
    return this.connectionToNode.get(connectionId);
  }

  /**
   * 通过 nodeId 获取 connectionId
   */
  getConnectionId(nodeId: string): string | undefined {
    return this.nodeToConnection.get(nodeId);
  }

  /**
   * 处理 link-down 事件
   * 将 connectionId 列表映射为 nodeId 列表
   */
  handleLinkDown(connectionId: string, affectedChildren: string[]): string[] {
    const nodeIds: string[] = [];

    // 映射主连接
    const nodeId = this.connectionToNode.get(connectionId);
    if (nodeId) nodeIds.push(nodeId);

    // 映射所有受影响的子连接
    for (const childConnId of affectedChildren) {
      const childNodeId = this.connectionToNode.get(childConnId);
      if (childNodeId) nodeIds.push(childNodeId);
    }

    return nodeIds;
  }

  /**
   * 处理重连成功事件
   */
  handleReconnected(connectionId: string): string | null {
    return this.connectionToNode.get(connectionId) || null;
  }

  /**
   * 清理所有映射（应用退出时）
   */
  clear(): void {
    this.connectionToNode.clear();
    this.nodeToConnection.clear();
  }

  /**
   * 获取当前映射数量（调试用）
   */
  size(): number {
    return this.connectionToNode.size;
  }

  /**
   * 调试：打印所有映射
   */
  dump(): void {
    console.log('[TopologyResolver] Current mappings:');
    this.connectionToNode.forEach((nodeId, connId) => {
      console.log(`  ${connId} -> ${nodeId}`);
    });
  }
}

export const topologyResolver = new TopologyResolver();
