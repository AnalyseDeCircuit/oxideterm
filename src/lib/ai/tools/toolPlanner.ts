// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolDefinition } from '../providers';
import type { TabType } from '../../../types';
import type { ToolIntent } from './toolDefinitions';
import {
  getToolDefinitionByName,
  getToolsForContext,
} from './toolDefinitions';

export type ToolIntentInferenceInput = {
  text: string;
  activeTabType?: TabType | null;
};

export type ToolPlanInput = {
  activeTabType: TabType | null;
  hasAnySSHSession: boolean;
  disabledTools?: Set<string>;
  participantOverride?: Set<string>;
  intents?: Iterable<ToolIntent>;
  userMessage?: string;
};

const CORE_TOOL_NAMES = [
  'list_targets',
  'list_capabilities',
  'list_tabs',
] as const;

const CONNECTION_INTENT_TOOL_NAMES = [
  'list_targets',
  'list_capabilities',
  'connect_saved_connection_by_query',
  'list_saved_connections',
  'search_saved_connections',
  'connect_saved_session',
  'get_session_tree',
  'get_ssh_environment',
  'get_topology',
] as const;

const SETTINGS_INTENT_TOOL_NAMES = [
  'open_tab',
  'open_settings_section',
  'get_settings',
  'update_setting',
] as const;

const CONNECTION_PATTERNS = [
  /\bssh\b/i,
  /\bconnect(?:ion)?\b/i,
  /\bsaved\s+(?:host|connection|session)\b/i,
  /\bhost\b/i,
  /\bserver\b/i,
  /\bjump\s*host\b/i,
  /\b(?:open|start|attach|进入|打开|连接|连上|连到|登录|登陆).*(?:主机|服务器|连接|ssh|host|server|session)\b/i,
  /(?:主机|服务器|保存连接|已保存连接|连接配置|会话|跳板机|堡垒机|内网机器|家里|公司).*(?:连接|打开|进入|登录|登陆|ssh)/i,
  /(?:连接|打开|进入|登录|登陆|ssh).*(?:主机|服务器|保存连接|已保存连接|连接配置|会话|跳板机|堡垒机|内网机器|家里|公司)/i,
];

const SETTINGS_PATTERNS = [
  /\bsettings?\b/i,
  /\bpreferences?\b/i,
  /\bconfig(?:uration)?\b/i,
  /\btheme\b/i,
  /\bfont\b/i,
  /\brenderer\b/i,
  /\bwebgl\b/i,
  /\bcanvas\b/i,
  /\bprovider\b/i,
  /\bmodel\b/i,
  /\breasoning\b/i,
  /\bsftp\b.*\b(?:parallel|concurrency|concurrent)\b/i,
  /(?:设置|配置|偏好|主题|字体|字号|渲染|提供商|模型上下文|上下文窗口|推理深度|思考深度|快捷键|高亮规则|并行|并发|限速)/i,
  /(?:修改|更改|改成|设置为|调整|开启|关闭|启用|禁用).*(?:设置|配置|主题|字体|字号|渲染|提供商|模型|上下文|推理|思考|快捷键|高亮规则|并行|并发|限速|SFTP)/i,
];

function matchesAny(text: string, patterns: readonly RegExp[]): boolean {
  return patterns.some((pattern) => pattern.test(text));
}

export function inferToolIntents(input: ToolIntentInferenceInput | string): ToolIntent[] {
  const text = typeof input === 'string' ? input : input.text;
  const activeTabType = typeof input === 'string' ? null : input.activeTabType;
  const normalized = text.trim();
  const intents = new Set<ToolIntent>();

  if (matchesAny(normalized, CONNECTION_PATTERNS)) {
    intents.add('connection');
  }

  if (matchesAny(normalized, SETTINGS_PATTERNS)) {
    intents.add('settings');
  }

  if (activeTabType === 'settings') {
    intents.add('settings');
  }

  if (activeTabType === 'session_manager' || activeTabType === 'connection_pool' || activeTabType === 'connection_monitor') {
    intents.add('connection');
  }

  return [...intents];
}

function addToolByName(
  definitions: AiToolDefinition[],
  seen: Set<string>,
  toolName: string,
  disabledTools?: Set<string>,
): void {
  if (seen.has(toolName) || disabledTools?.has(toolName)) return;
  const definition = getToolDefinitionByName(toolName);
  if (!definition) return;
  definitions.push(definition);
  seen.add(toolName);
}

export function getToolsForPlan(input: ToolPlanInput): AiToolDefinition[] {
  const inferredIntents = input.intents
    ? [...input.intents]
    : input.userMessage
      ? inferToolIntents({ text: input.userMessage, activeTabType: input.activeTabType })
      : [];
  const intentSet = new Set(inferredIntents);
  const definitions = getToolsForContext(
    input.activeTabType,
    input.hasAnySSHSession,
    input.disabledTools,
    input.participantOverride,
  );
  const seen = new Set(definitions.map((tool) => tool.name));

  for (const toolName of CORE_TOOL_NAMES) {
    addToolByName(definitions, seen, toolName, input.disabledTools);
  }

  if (intentSet.has('connection')) {
    for (const toolName of CONNECTION_INTENT_TOOL_NAMES) {
      addToolByName(definitions, seen, toolName, input.disabledTools);
    }
  }

  if (intentSet.has('settings')) {
    for (const toolName of SETTINGS_INTENT_TOOL_NAMES) {
      addToolByName(definitions, seen, toolName, input.disabledTools);
    }
  }

  return definitions;
}
