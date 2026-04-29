import { describe, expect, it } from 'vitest';

import {
  ALL_BUILTIN_TOOL_DEFS,
  CONTEXT_FREE_TOOLS,
  EXPERIMENTAL_TOOLS,
  IDE_ONLY_TOOLS,
  LOCAL_ONLY_TOOLS,
  MONITOR_ONLY_TOOLS,
  PLUGIN_MGR_ONLY_TOOLS,
  POOL_ONLY_TOOLS,
  READ_ONLY_TOOLS,
  SESSION_ID_TOOLS,
  SESSION_MGR_ONLY_TOOLS,
  SETTINGS_ONLY_TOOLS,
  SFTP_ONLY_TOOLS,
  SSH_ONLY_TOOLS,
  TOOL_SPEC_BY_NAME,
  TOOL_GROUPS,
  WRITE_TOOLS,
  getAllToolSpecs,
  getToolDefinitionByName,
  getToolSpec,
  getToolsForPlan,
  getToolsForContext,
  inferToolIntents,
  classifyToolObligation,
  pluginManifestToAiToolSpecs,
  scoreToolsForRequest,
} from '@/lib/ai/tools';
import type { PluginManifest } from '@/types/plugin';

function toolNamesForContext(...args: Parameters<typeof getToolsForContext>): Set<string> {
  return new Set(getToolsForContext(...args).map((tool) => tool.name));
}

describe('tool registry v3 compatibility layer', () => {
  it('registers every built-in tool exactly once', () => {
    const definitions = ALL_BUILTIN_TOOL_DEFS;
    const definitionNames = definitions.map((tool) => tool.name);
    const uniqueDefinitionNames = new Set(definitionNames);

    expect(uniqueDefinitionNames.size).toBe(definitionNames.length);
    expect(getAllToolSpecs()).toHaveLength(definitions.length);
    expect(TOOL_SPEC_BY_NAME.size).toBe(definitions.length);

    for (const definition of definitions) {
      const spec = getToolSpec(definition.name);
      expect(spec?.definition).toBe(definition);
      expect(getToolDefinitionByName(definition.name)).toBe(definition);
    }
  });

  it('preserves legacy classification sets on generated specs', () => {
    for (const spec of getAllToolSpecs()) {
      expect(spec.readOnly).toBe(READ_ONLY_TOOLS.has(spec.name));
      expect(spec.write).toBe(WRITE_TOOLS.has(spec.name));
      expect(spec.contextFree).toBe(CONTEXT_FREE_TOOLS.has(spec.name));
      expect(spec.sessionIdTool).toBe(SESSION_ID_TOOLS.has(spec.name));
      expect(spec.experimental).toBe(EXPERIMENTAL_TOOLS.has(spec.name));
    }
  });

  it('does not keep stale tool names in classification metadata', () => {
    const knownTools = new Set(ALL_BUILTIN_TOOL_DEFS.map((tool) => tool.name));
    const classificationSets = [
      READ_ONLY_TOOLS,
      WRITE_TOOLS,
      EXPERIMENTAL_TOOLS,
      CONTEXT_FREE_TOOLS,
      SESSION_ID_TOOLS,
      SSH_ONLY_TOOLS,
      SFTP_ONLY_TOOLS,
      IDE_ONLY_TOOLS,
      LOCAL_ONLY_TOOLS,
      SETTINGS_ONLY_TOOLS,
      POOL_ONLY_TOOLS,
      MONITOR_ONLY_TOOLS,
      SESSION_MGR_ONLY_TOOLS,
      PLUGIN_MGR_ONLY_TOOLS,
    ];

    for (const set of classificationSets) {
      for (const toolName of set) {
        expect(knownTools.has(toolName), `${toolName} should be a registered built-in tool`).toBe(true);
      }
    }

    for (const group of TOOL_GROUPS) {
      for (const toolName of [...group.readOnly, ...group.write]) {
        expect(knownTools.has(toolName), `${toolName} in group ${group.groupKey} should be registered`).toBe(true);
      }
    }
  });

  it('keeps tab-specific visibility compatible with the legacy filter', () => {
    const noTabTools = toolNamesForContext(null, false);
    const settingsTools = toolNamesForContext('settings', false);
    const sessionManagerTools = toolNamesForContext('session_manager', false);
    const sftpTools = toolNamesForContext('sftp', true);

    expect(noTabTools.has('get_settings')).toBe(false);
    expect(noTabTools.has('search_saved_connections')).toBe(false);
    expect(settingsTools.has('get_settings')).toBe(true);
    expect(settingsTools.has('update_setting')).toBe(true);
    expect(sessionManagerTools.has('search_saved_connections')).toBe(true);
    expect(sessionManagerTools.has('get_session_tree')).toBe(true);
    expect(sftpTools.has('sftp_list_dir')).toBe(true);
  });

  it('keeps participant overrides and disabled tools precedence unchanged', () => {
    const overridden = toolNamesForContext(null, false, undefined, new Set(['search_saved_connections']));
    const disabled = toolNamesForContext('settings', false, new Set(['get_settings']), new Set(['get_settings']));

    expect(overridden.has('search_saved_connections')).toBe(true);
    expect(disabled.has('get_settings')).toBe(false);
  });
});

describe('tool disclosure planner v3 phase 2', () => {
  it('infers connection intent from saved-host requests', () => {
    expect(inferToolIntents('连接家里的主机本地')).toContain('connection');
    expect(inferToolIntents('open my saved SSH connection')).toContain('connection');
    expect(inferToolIntents('看看现在有哪些远程主机可供链接')).toContain('connection');
  });

  it('infers settings intent from configuration requests', () => {
    expect(inferToolIntents('把 SFTP 并行数量改成 4')).toContain('settings');
    expect(inferToolIntents('change terminal renderer to canvas')).toContain('settings');
  });

  it('infers non-tab intents for terminal, local, sftp, ide, and knowledge requests', () => {
    expect(inferToolIntents('在本地执行 pwd')).toEqual(expect.arrayContaining(['local_shell', 'command']));
    expect(inferToolIntents('读取当前终端缓冲区')).toContain('terminal_interaction');
    expect(inferToolIntents('用 SFTP 上传这个目录')).toContain('sftp');
    expect(inferToolIntents('打开 IDE 里的当前文件')).toContain('ide');
    expect(inferToolIntents('搜索知识库里的插件开发文档')).toContain('knowledge');
  });

  it('exposes connection workflow tools outside session manager when intent matches', () => {
    const names = getToolsForPlan({
      activeTabType: 'local_terminal',
      hasAnySSHSession: false,
      userMessage: '连接家里的主机本地',
    }).map((tool) => tool.name);
    const tools = new Set(names);

    expect(names[0]).toBe('resolve_target');
    expect(tools.has('open_local_terminal')).toBe(false);
    expect(tools.has('search_saved_connections')).toBe(true);
    expect(tools.has('connect_saved_connection_by_query')).toBe(true);
    expect(tools.has('connect_saved_session')).toBe(true);
    expect(tools.has('get_session_tree')).toBe(true);
  });

  it('exposes settings tools outside settings tab when intent matches', () => {
    const tools = new Set(getToolsForPlan({
      activeTabType: null,
      hasAnySSHSession: false,
      userMessage: '修改终端字体大小',
    }).map((tool) => tool.name));

    expect(tools.has('open_tab')).toBe(true);
    expect(tools.has('open_settings_section')).toBe(true);
    expect(tools.has('get_settings')).toBe(true);
    expect(tools.has('update_setting')).toBe(true);
  });

  it('still respects disabled tools for intent-expanded tools', () => {
    const tools = new Set(getToolsForPlan({
      activeTabType: null,
      hasAnySSHSession: false,
      userMessage: '连接家里的主机本地',
      disabledTools: new Set(['connect_saved_session']),
    }).map((tool) => tool.name));

    expect(tools.has('search_saved_connections')).toBe(true);
    expect(tools.has('connect_saved_session')).toBe(false);
  });

  it('scores workflow tools ahead of generic discovery tools for explicit actions', () => {
    const scores = scoreToolsForRequest({
      activeTabType: null,
      hasAnySSHSession: false,
      userMessage: '连接家里的主机本地',
    });
    const names = scores.map((score) => score.toolName);

    expect(names[0]).toBe('resolve_target');
    expect(names.indexOf('search_saved_connections')).toBeGreaterThanOrEqual(0);
    expect(names.indexOf('connect_saved_connection_by_query')).toBeGreaterThanOrEqual(0);
    expect(names.indexOf('search_saved_connections')).toBeLessThan(names.indexOf('list_targets'));
  });

  it('routes broad remote-host discovery to saved connection listing', () => {
    const names = getToolsForPlan({
      activeTabType: 'local_terminal',
      hasAnySSHSession: false,
      userMessage: '看看现在有哪些远程主机可供链接',
    }).map((tool) => tool.name);

    expect(names[0]).toBe('list_saved_connections');
    expect(names.indexOf('list_saved_connections')).toBeLessThan(names.indexOf('resolve_target'));
    expect(names.indexOf('search_saved_connections')).toBeGreaterThanOrEqual(0);
  });
});

describe('tool obligation classifier v4', () => {
  it('requires tools for live app actions', () => {
    const obligation = classifyToolObligation({
      text: '打开设置并把终端 renderer 改成 canvas',
      activeTabType: null,
      availableToolNames: ['get_settings', 'open_settings_section', 'update_setting'],
    });

    expect(obligation.mode).toBe('required');
    expect(obligation.candidateTools).toEqual(expect.arrayContaining(['update_setting']));
  });

  it('requires tools for saved connection workflows outside the session manager', () => {
    const obligation = classifyToolObligation({
      text: '连接家里的主机本地',
      activeTabType: 'local_terminal',
      availableToolNames: ['search_saved_connections', 'connect_saved_connection_by_query'],
    });

    expect(obligation.mode).toBe('required');
    expect(obligation.intents).toContain('connection');
    expect(obligation.candidateTools).toContain('connect_saved_connection_by_query');
  });

  it('requires list tools for broad remote host discovery', () => {
    const obligation = classifyToolObligation({
      text: '看看现在有哪些远程主机可供链接',
      activeTabType: 'local_terminal',
      availableToolNames: ['resolve_target', 'list_saved_connections', 'search_saved_connections'],
    });

    expect(obligation.mode).toBe('required');
    expect(obligation.intents).toContain('connection');
    expect(obligation.candidateTools[0]).toBe('list_saved_connections');
  });

  it('does not force tools for conceptual architecture discussion', () => {
    const obligation = classifyToolObligation({
      text: '你觉得终端工具调用系统应该怎么设计',
      activeTabType: null,
      availableToolNames: ['list_targets', 'terminal_exec'],
    });

    expect(obligation.mode).not.toBe('required');
  });

  it('treats pasted evidence explanations as optional tool use', () => {
    const obligation = classifyToolObligation({
      text: '解释这段报错：```ssh: connect to host 1.2.3.4 port 22: Connection refused```',
      activeTabType: null,
      availableToolNames: ['search_docs', 'list_targets'],
    });

    expect(obligation.mode).toBe('optional');
  });
});

describe('external tool spec adapters', () => {
  it('maps plugin AI tool metadata into v3 tool specs without changing the plugin API', () => {
    const manifest: PluginManifest = {
      id: 'com.example.plugin',
      name: 'Example',
      version: '1.0.0',
      main: 'index.js',
      contributes: {
        aiTools: [
          {
            name: 'open_thing',
            description: 'Open a plugin thing',
            parameters: { type: 'object', properties: {} },
            capabilities: ['navigation.open'],
            targetKinds: ['app-tab'],
          },
          {
            name: 'read_thing',
            description: 'Read a plugin thing',
            capabilities: ['state.list'],
          },
        ],
      },
    };

    const specs = pluginManifestToAiToolSpecs(manifest);

    expect(specs.map((spec) => spec.name)).toEqual([
      'plugin::com.example.plugin::open_thing',
      'plugin::com.example.plugin::read_thing',
    ]);
    expect(specs[0]).toMatchObject({
      domain: 'plugin',
      sideEffect: 'navigate',
      requiredTarget: 'app_tab',
      write: true,
    });
    expect(specs[1]).toMatchObject({
      domain: 'plugin',
      sideEffect: 'read',
      requiredTarget: 'none',
      readOnly: true,
    });
  });
});
