/**
 * ToolIndicator — Shows active tool count badge next to ModelSelector.
 * Opens a popover allowing users to enable/disable tools per-group and per-tool.
 * Supports global persistence (settingsStore) + session-level override (aiChatStore).
 */

import { useState, useRef, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Wrench, ChevronDown, ChevronRight, RotateCcw,
  Terminal as TerminalIcon, Network, Radio, FolderInput, Code2,
  Activity, Puzzle, Settings, Eye, EyeOff,
} from 'lucide-react';
import { useSettingsStore } from '../../store/settingsStore';
import { useAiChatStore } from '../../store/aiChatStore';
import { TOOL_GROUPS, getToolsForContext } from '../../lib/ai/tools';
import { cn } from '../../lib/utils';
import type { TabType } from '../../types';

/** Group icon map — mirrors SettingsView's TOOL_GROUP_ICONS */
const GROUP_ICONS: Record<string, React.ElementType> = {
  terminal: TerminalIcon, session: Network, infrastructure: Radio, sftp: FolderInput, ide: Code2,
  local_terminal: TerminalIcon, settings: Settings, connection_pool: Activity,
  connection_monitor: Activity, session_manager: Network, plugin_manager: Puzzle,
};

type ToolIndicatorProps = {
  /** Current active tab type for context filtering */
  activeTabType: TabType | null;
  /** Whether any SSH session exists */
  hasAnySSHSession: boolean;
};

export const ToolIndicator = ({ activeTabType, hasAnySSHSession }: ToolIndicatorProps) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const popoverRef = useRef<HTMLDivElement>(null);

  const toolUseEnabled = useSettingsStore((s) => s.settings.ai.toolUse?.enabled === true);
  const updateAi = useSettingsStore((s) => s.updateAi);
  const toolUse = useSettingsStore((s) => s.settings.ai.toolUse);

  const sessionDisabled = useAiChatStore((s) => s.sessionDisabledTools);
  const setSessionDisabled = useAiChatStore((s) => s.setSessionDisabledTools);

  // Effective disabled set: session override or global
  const disabledSet = useMemo(() => {
    if (sessionDisabled !== null) return new Set(sessionDisabled);
    return new Set(toolUse?.disabledTools ?? []);
  }, [sessionDisabled, toolUse?.disabledTools]);

  // All tools available for current context (before disabled filter)
  const contextTools = useMemo(
    () => getToolsForContext(activeTabType, hasAnySSHSession),
    [activeTabType, hasAnySSHSession],
  );

  // Active tools = context tools minus disabled
  const activeTools = useMemo(
    () => contextTools.filter((tool) => !disabledSet.has(tool.name)),
    [contextTools, disabledSet],
  );

  const contextToolNames = useMemo(
    () => new Set(contextTools.map((tool) => tool.name)),
    [contextTools],
  );

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [open]);

  // Toggle a single tool
  const toggleTool = useCallback((toolName: string) => {
    const newSet = new Set(disabledSet);
    if (newSet.has(toolName)) {
      newSet.delete(toolName);
    } else {
      newSet.add(toolName);
    }
    const arr = [...newSet];

    if (sessionDisabled !== null) {
      setSessionDisabled(arr);
    } else {
      updateAi('toolUse', { ...(toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), disabledTools: arr });
    }
  }, [disabledSet, sessionDisabled, setSessionDisabled, updateAi, toolUse]);

  // Toggle an entire group
  const toggleGroup = useCallback((groupKey: string) => {
    const group = TOOL_GROUPS.find((g) => g.groupKey === groupKey);
    if (!group) return;

    const groupTools = [...group.readOnly, ...group.write].filter((n) => contextToolNames.has(n));
    if (groupTools.length === 0) return;

    const allEnabled = groupTools.every((n) => !disabledSet.has(n));
    const newSet = new Set(disabledSet);

    if (allEnabled) {
      // Disable entire group
      for (const n of groupTools) newSet.add(n);
    } else {
      // Enable entire group
      for (const n of groupTools) newSet.delete(n);
    }
    const arr = [...newSet];

    if (sessionDisabled !== null) {
      setSessionDisabled(arr);
    } else {
      updateAi('toolUse', { ...(toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), disabledTools: arr });
    }
  }, [disabledSet, contextToolNames, sessionDisabled, setSessionDisabled, updateAi, toolUse]);

  // Toggle session override mode
  const toggleSessionOverride = useCallback(() => {
    if (sessionDisabled !== null) {
      // Clear session override → fall back to global
      setSessionDisabled(null);
    } else {
      // Start session override with current global state
      setSessionDisabled([...(toolUse?.disabledTools ?? [])]);
    }
  }, [sessionDisabled, setSessionDisabled, toolUse?.disabledTools]);

  // Don't render if tool use is disabled
  if (!toolUseEnabled) return null;

  const isUsingSessionOverride = sessionDisabled !== null;

  return (
    <div className="relative" ref={popoverRef}>
      {/* Trigger badge */}
      <button
        onClick={() => setOpen(!open)}
        className={cn(
          "flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium",
          "text-theme-text-muted hover:text-theme-text hover:bg-theme-accent/10",
          open && "bg-theme-accent/10 text-theme-text",
          isUsingSessionOverride && "ring-1 ring-amber-400/40",
        )}
        title={t('ai.tool_selector.tooltip', { count: activeTools.length, total: contextTools.length })}
      >
        <Wrench className="w-2.5 h-2.5 shrink-0" />
        <span>{activeTools.length}/{contextTools.length}</span>
        <ChevronDown className="w-2.5 h-2.5 shrink-0" />
      </button>

      {/* Popover */}
      {open && (
        <div className="absolute left-0 bottom-full mb-0.5 w-72 bg-theme-bg-panel border border-theme-border shadow-lg z-50 overflow-hidden">
          {/* Header */}
          <div className="flex items-center justify-between px-3 py-1.5 border-b border-theme-border/30">
            <span className="text-[10px] font-bold tracking-wider uppercase text-theme-text-muted">
              {t('ai.tool_selector.title')}
            </span>
            <div className="flex items-center gap-1.5">
              {/* Session override toggle */}
              <button
                onClick={toggleSessionOverride}
                className={cn(
                  "flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px]",
                  isUsingSessionOverride
                    ? "text-amber-400 bg-amber-400/10 hover:bg-amber-400/20"
                    : "text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover/50",
                )}
                title={t(isUsingSessionOverride ? 'ai.tool_selector.session_override_active' : 'ai.tool_selector.session_override_off')}
              >
                {isUsingSessionOverride ? <EyeOff className="w-2.5 h-2.5" /> : <Eye className="w-2.5 h-2.5" />}
                <span>{t(isUsingSessionOverride ? 'ai.tool_selector.session' : 'ai.tool_selector.global')}</span>
              </button>

              {/* Reset button */}
              <button
                onClick={() => {
                  if (sessionDisabled !== null) {
                    setSessionDisabled([]);
                  } else {
                    updateAi('toolUse', { ...(toolUse ?? { enabled: false, autoApproveTools: {}, disabledTools: [] }), disabledTools: [] });
                  }
                }}
                className="p-0.5 text-theme-text-muted hover:text-theme-text"
                title={t('ai.tool_selector.reset')}
              >
                <RotateCcw className="w-2.5 h-2.5" />
              </button>
            </div>
          </div>

          {/* Tool groups */}
          <div className="max-h-80 overflow-y-auto py-1">
            {TOOL_GROUPS.map((group) => {
              const GroupIcon = GROUP_ICONS[group.groupKey] ?? Wrench;
              const groupTools = [...group.readOnly, ...group.write].filter((n) => contextToolNames.has(n));

              // Skip groups with no tools in current context
              if (groupTools.length === 0) return null;

              const enabledCount = groupTools.filter((n) => !disabledSet.has(n)).length;
              const allEnabled = enabledCount === groupTools.length;
              const noneEnabled = enabledCount === 0;
              const isExpanded = expandedGroups.has(group.groupKey);

              return (
                <div key={group.groupKey} className="border-b border-theme-border/10 last:border-b-0">
                  {/* Group header — toggles group enable/disable */}
                  <div className="flex items-center gap-1.5 px-3 py-1.5 hover:bg-theme-bg-hover/30">
                    <button
                      onClick={() => {
                        setExpandedGroups((prev) => {
                          const next = new Set(prev);
                          if (next.has(group.groupKey)) next.delete(group.groupKey);
                          else next.add(group.groupKey);
                          return next;
                        });
                      }}
                      className="flex items-center gap-1.5 flex-1 min-w-0 text-left"
                    >
                      {isExpanded
                        ? <ChevronDown className="w-2.5 h-2.5 shrink-0 text-theme-text-muted" />
                        : <ChevronRight className="w-2.5 h-2.5 shrink-0 text-theme-text-muted" />
                      }
                      <GroupIcon className="w-3 h-3 shrink-0 text-theme-text-muted" />
                      <span className="text-[11px] font-medium text-theme-text truncate">
                        {t(`settings_view.ai.tool_use_group_${group.groupKey}`)}
                      </span>
                      <span className="text-[10px] text-theme-text-muted ml-auto shrink-0">
                        {enabledCount}/{groupTools.length}
                      </span>
                    </button>
                    {/* Group toggle switch */}
                    <button
                      onClick={() => toggleGroup(group.groupKey)}
                      className={cn(
                        "w-7 h-3.5 rounded-full relative shrink-0 transition-colors",
                        allEnabled ? "bg-theme-accent" : noneEnabled ? "bg-theme-border" : "bg-theme-accent/50",
                      )}
                      title={allEnabled ? t('ai.tool_selector.disable_group') : t('ai.tool_selector.enable_group')}
                    >
                      <div className={cn(
                        "absolute top-0.5 w-2.5 h-2.5 rounded-full bg-white transition-transform",
                        allEnabled ? "translate-x-3.5" : "translate-x-0.5",
                      )} />
                    </button>
                  </div>

                  {/* Expanded per-tool list */}
                  {isExpanded && (
                    <div className="pl-7 pr-3 pb-1.5">
                      {groupTools.map((toolName) => {
                        const isEnabled = !disabledSet.has(toolName);
                        return (
                          <button
                            key={toolName}
                            onClick={() => toggleTool(toolName)}
                            className={cn(
                              "flex items-center gap-2 w-full text-left px-2 py-1 rounded text-[10px] transition-colors",
                              isEnabled
                                ? "text-theme-text hover:bg-theme-accent/10"
                                : "text-theme-text-muted/50 hover:bg-theme-bg-hover/30 line-through",
                            )}
                          >
                            <div className={cn(
                              "w-2 h-2 rounded-full shrink-0",
                              isEnabled ? "bg-theme-accent" : "bg-theme-border",
                            )} />
                            <span className="truncate">
                              {t(`ai.tool_use.tool_names.${toolName}`, { defaultValue: toolName })}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>

          {/* Footer: active count summary */}
          <div className="px-3 py-1.5 border-t border-theme-border/30 text-[10px] text-theme-text-muted">
            {t('ai.tool_selector.active_summary', { active: activeTools.length, total: contextTools.length })}
            {isUsingSessionOverride && (
              <span className="ml-1 text-amber-400">({t('ai.tool_selector.session')})</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
