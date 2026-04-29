// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolDefinition } from '../providers';
import type { TabType } from '../../../types';
import type { ToolIntent } from './toolDefinitions';
import {
  getAllToolSpecs,
  getToolDefinitionByName,
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
  'resolve_target',
  'list_capabilities',
] as const;

const CONNECTION_INTENT_TOOL_NAMES = [
  'resolve_target',
  'list_capabilities',
  'connect_saved_connection_by_query',
  'list_saved_connections',
  'search_saved_connections',
  'connect_saved_session',
  'get_session_tree',
  'get_ssh_environment',
  'get_topology',
] as const;

const CONNECTION_DISCOVERY_TOOL_NAMES = [
  'list_saved_connections',
  'search_saved_connections',
  'get_session_tree',
  'get_topology',
  'list_connections',
] as const;

const SETTINGS_INTENT_TOOL_NAMES = [
  'open_tab',
  'open_settings_section',
  'get_settings',
  'update_setting',
] as const;

const INTENT_TOOL_NAMES: Record<ToolIntent, readonly string[]> = {
  command: [
    'resolve_target',
    'list_capabilities',
    'terminal_exec',
    'local_exec',
    'get_terminal_buffer',
    'read_screen',
  ],
  terminal_interaction: [
    'resolve_target',
    'list_capabilities',
    'get_terminal_buffer',
    'read_screen',
    'await_terminal_output',
    'terminal_exec',
    'send_keys',
    'send_control_sequence',
  ],
  connection: CONNECTION_INTENT_TOOL_NAMES,
  settings: SETTINGS_INTENT_TOOL_NAMES,
  remote_file: [
    'resolve_target',
    'read_file',
    'list_directory',
    'grep_search',
    'git_status',
    'write_file',
    'ide_get_open_files',
    'ide_get_file_content',
    'ide_open_file',
  ],
  local_shell: [
    'resolve_target',
    'local_list_shells',
    'local_get_terminal_info',
    'local_get_drives',
    'local_exec',
    'open_local_terminal',
    'terminal_exec',
  ],
  sftp: [
    'resolve_target',
    'open_session_tab',
    'sftp_get_cwd',
    'sftp_list_dir',
    'sftp_stat',
    'sftp_read_file',
    'sftp_write_file',
  ],
  ide: [
    'resolve_target',
    'ide_get_project_info',
    'ide_get_open_files',
    'ide_get_file_content',
    'ide_open_file',
    'ide_replace_string',
    'ide_insert_text',
    'ide_create_file',
  ],
  monitoring: [
    'get_all_health',
    'get_resource_metrics',
    'get_connection_health',
    'list_connections',
    'get_pool_stats',
  ],
  navigation: [
    'resolve_target',
    'list_tabs',
    'open_tab',
    'open_session_tab',
    'open_local_terminal',
    'open_settings_section',
  ],
  plugin: [
    'list_plugins',
    'get_plugin_details',
  ],
  knowledge: [
    'search_docs',
    'list_mcp_resources',
    'read_mcp_resource',
  ],
  status: [
    'resolve_target',
    'list_targets',
    'list_tabs',
    'get_event_log',
    'get_transfer_status',
    'get_recording_status',
    'get_broadcast_status',
    'get_ssh_environment',
    'get_topology',
    'get_session_tree',
  ],
};

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

const CONNECTION_DISCOVERY_PATTERNS = [
  /\b(?:list|show|what|which|available|saved)\b.*\b(?:hosts?|servers?|connections?|sessions?)\b/i,
  /\b(?:hosts?|servers?|connections?|sessions?)\b.*\b(?:available|saved|configured)\b/i,
  /(?:有哪些|有什么|列出|查看|显示|可用|保存的|已保存).*(?:远程主机|主机|服务器|连接|SSH|ssh|会话)/i,
  /(?:远程主机|主机|服务器|保存连接|已保存连接|连接配置|SSH|ssh|会话).*(?:有哪些|有什么|列表|列出|查看|显示|可用)/i,
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

const COMMAND_PATTERNS = [
  /\b(?:run|execute|exec|launch|start)\b.*\b(?:command|script|shell|terminal)\b/i,
  /\b(?:command|shell command)\b/i,
  /\b(?:pwd|ls|cat|grep|git|npm|pnpm|cargo|kubectl|docker|systemctl|ssh)\b/i,
  /(?:执行|运行|跑一下|命令|脚本|终端里运行|终端执行|查一下命令|跑命令)/i,
];

const TERMINAL_PATTERNS = [
  /\b(?:terminal|tty|shell|screen|buffer|prompt|sudo|password|passphrase|tui|vim|nvim|tmux)\b/i,
  /(?:当前终端|终端输出|终端缓冲区|屏幕|提示符|密码|输入|按键|控制信号|Ctrl-|sudo|vim|nvim|tmux|TUI)/i,
];

const LOCAL_SHELL_PATTERNS = [
  /\b(?:local|localhost|my machine|this machine|mac|windows|linux)\b.*\b(?:terminal|shell|command|file|process)\b/i,
  /(?:本地终端|本机|我的电脑|这台机器|本地执行|打开本地终端|本地命令|本地 shell)/i,
];

const REMOTE_FILE_PATTERNS = [
  /\b(?:read|write|edit|open|grep|search|list)\b.*\b(?:file|directory|folder|repo|nginx|config)\b/i,
  /\b(?:file|directory|folder|path|repo|nginx\.conf)\b/i,
  /(?:读取文件|写入文件|修改文件|编辑文件|打开文件|列目录|搜索文件|查找文件|远程文件|配置文件|nginx 配置)/i,
];

const SFTP_PATTERNS = [
  /\bsftp\b/i,
  /\b(?:upload|download|transfer|remote file manager)\b/i,
  /(?:SFTP|上传|下载|传输|远程文件管理|文件管理器|目录传输)/i,
];

const IDE_PATTERNS = [
  /\b(?:ide|editor|open file|replace string|insert text|project info)\b/i,
  /(?:IDE|编辑器|打开项目|替换字符串|插入文本|项目文件|当前打开文件)/i,
];

const MONITORING_PATTERNS = [
  /\b(?:monitor|metrics|cpu|memory|health|latency|pool|resource)\b/i,
  /(?:监控|指标|CPU|内存|健康|连接池|资源|延迟)/i,
];

const NAVIGATION_PATTERNS = [
  /\b(?:open|switch|show|focus)\b.*\b(?:tab|panel|sidebar|view|page)\b/i,
  /(?:打开|切到|显示|聚焦).*(?:标签页|侧边栏|面板|页面|视图)/i,
];

const PLUGIN_PATTERNS = [
  /\bplugins?\b/i,
  /(?:插件|plugin)/i,
];

const KNOWLEDGE_PATTERNS = [
  /\b(?:docs|documentation|knowledge|rag|search docs)\b/i,
  /(?:文档|知识库|RAG|搜索文档|查文档)/i,
];

const STATUS_PATTERNS = [
  /\b(?:status|state|event log|recording|broadcast|topology|environment)\b/i,
  /(?:状态|事件日志|录制|广播|拓扑|环境|当前情况|健康状态)/i,
];

function matchesAny(text: string, patterns: readonly RegExp[]): boolean {
  return patterns.some((pattern) => pattern.test(text));
}

export function isConnectionDiscoveryRequest(text: string): boolean {
  return matchesAny(text.trim(), CONNECTION_DISCOVERY_PATTERNS);
}

export function inferToolIntents(input: ToolIntentInferenceInput | string): ToolIntent[] {
  const text = typeof input === 'string' ? input : input.text;
  const activeTabType = typeof input === 'string' ? null : input.activeTabType;
  const normalized = text.trim();
  const intents = new Set<ToolIntent>();

  if (matchesAny(normalized, CONNECTION_PATTERNS) || isConnectionDiscoveryRequest(normalized)) {
    intents.add('connection');
  }

  if (matchesAny(normalized, SETTINGS_PATTERNS)) {
    intents.add('settings');
  }

  if (matchesAny(normalized, COMMAND_PATTERNS)) {
    intents.add('command');
  }

  if (matchesAny(normalized, TERMINAL_PATTERNS)) {
    intents.add('terminal_interaction');
  }

  if (matchesAny(normalized, LOCAL_SHELL_PATTERNS)) {
    intents.add('local_shell');
  }

  if (matchesAny(normalized, REMOTE_FILE_PATTERNS)) {
    intents.add('remote_file');
  }

  if (matchesAny(normalized, SFTP_PATTERNS)) {
    intents.add('sftp');
  }

  if (matchesAny(normalized, IDE_PATTERNS)) {
    intents.add('ide');
  }

  if (matchesAny(normalized, MONITORING_PATTERNS)) {
    intents.add('monitoring');
  }

  if (matchesAny(normalized, NAVIGATION_PATTERNS)) {
    intents.add('navigation');
  }

  if (matchesAny(normalized, PLUGIN_PATTERNS)) {
    intents.add('plugin');
  }

  if (matchesAny(normalized, KNOWLEDGE_PATTERNS)) {
    intents.add('knowledge');
  }

  if (matchesAny(normalized, STATUS_PATTERNS)) {
    intents.add('status');
  }

  // UI focus is only a hint. Do not let the active tab expand capabilities when
  // the user's text already identifies a different task domain.
  if (intents.size === 0) {
    if (activeTabType === 'settings') {
      intents.add('settings');
    }

    if (activeTabType === 'local_terminal') {
      intents.add('local_shell');
      intents.add('terminal_interaction');
    }

    if (activeTabType === 'terminal') {
      intents.add('command');
      intents.add('terminal_interaction');
    }

    if (activeTabType === 'sftp') {
      intents.add('sftp');
    }

    if (activeTabType === 'ide') {
      intents.add('ide');
    }

    if (activeTabType === 'session_manager' || activeTabType === 'connection_pool' || activeTabType === 'connection_monitor') {
      intents.add('connection');
    }
  }

  return [...intents];
}

export type ToolScore = {
  toolName: string;
  score: number;
  reasons: string[];
};

export function scoreToolsForRequest(input: ToolPlanInput): ToolScore[] {
  const text = (input.userMessage ?? '').toLowerCase();
  const rawText = input.userMessage ?? '';
  const connectionDiscovery = isConnectionDiscoveryRequest(rawText);
  const intents = input.intents
    ? [...input.intents]
    : input.userMessage
      ? inferToolIntents({ text: input.userMessage, activeTabType: input.activeTabType })
      : [];
  const intentSet = new Set(intents);

  return getAllToolSpecs()
    .map((spec) => {
      let score = 0;
      const reasons: string[] = [];

      for (const intent of intentSet) {
        if (spec.intentTags.includes(intent)) {
          score += 8;
          reasons.push(`intent:${intent}`);
        }
      }

      if (spec.contextFree) {
        score += 1;
      }

      if (spec.name === 'resolve_target') {
        score += connectionDiscovery ? 1 : 10;
        reasons.push(connectionDiscovery ? 'target-first-not-for-listing' : 'target-first');
      }

      if (spec.name === 'list_targets' || spec.name === 'list_capabilities') {
        score += 3;
        reasons.push('discovery');
      }

      if (text.includes(spec.name.toLowerCase())) {
        score += 10;
        reasons.push('name-match');
      }

      if (intentSet.has('connection') && ['list_saved_connections', 'search_saved_connections', 'connect_saved_connection_by_query', 'connect_saved_session'].includes(spec.name)) {
        score += 8;
        reasons.push('connection-workflow');
      }

      if (connectionDiscovery && CONNECTION_DISCOVERY_TOOL_NAMES.includes(spec.name as typeof CONNECTION_DISCOVERY_TOOL_NAMES[number])) {
        score += 18;
        reasons.push('connection-discovery');
      }

      if (intentSet.has('settings') && ['get_settings', 'open_settings_section', 'update_setting'].includes(spec.name)) {
        score += 8;
        reasons.push('settings-workflow');
      }

      if (intentSet.has('local_shell') && spec.name === 'local_exec') {
        score += 6;
        reasons.push('local-one-shot');
      }

      if (intentSet.has('command') && spec.name === 'terminal_exec') {
        score += 6;
        reasons.push('command-exec');
      }

      if (intentSet.has('terminal_interaction') && ['read_screen', 'get_terminal_buffer'].includes(spec.name)) {
        score += 5;
        reasons.push('observe-first');
      }

      return { toolName: spec.name, score, reasons };
    })
    .filter((score) => score.score > 0 && !input.disabledTools?.has(score.toolName))
    .sort((a, b) => b.score - a.score || a.toolName.localeCompare(b.toolName));
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
  const definitions: AiToolDefinition[] = [];
  const seen = new Set<string>();

  for (const toolName of CORE_TOOL_NAMES) {
    addToolByName(definitions, seen, toolName, input.disabledTools);
  }

  if (input.participantOverride) {
    for (const toolName of input.participantOverride) {
      addToolByName(definitions, seen, toolName, input.disabledTools);
    }
  }

  if (intentSet.has('connection')) {
    for (const toolName of CONNECTION_INTENT_TOOL_NAMES) {
      addToolByName(definitions, seen, toolName, input.disabledTools);
    }
  }

  for (const intent of intentSet) {
    for (const toolName of INTENT_TOOL_NAMES[intent] ?? []) {
      addToolByName(definitions, seen, toolName, input.disabledTools);
    }
  }

  if (input.userMessage) {
    const scoreByName = new Map(scoreToolsForRequest(input).map((score) => [score.toolName, score.score]));
    const originalIndex = new Map(definitions.map((tool, index) => [tool.name, index]));
    definitions.sort((a, b) => {
      const scoreDiff = (scoreByName.get(b.name) ?? 0) - (scoreByName.get(a.name) ?? 0);
      return scoreDiff || ((originalIndex.get(a.name) ?? 0) - (originalIndex.get(b.name) ?? 0));
    });
  }

  return definitions;
}
