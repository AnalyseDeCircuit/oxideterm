// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useTranslation } from 'react-i18next';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Slider } from '@/components/ui/slider';
import { getFontFamily } from '@/lib/fontFamily';
import { platform } from '@/lib/platform';
import { TerminalHighlightRulesSection } from '@/components/settings/TerminalHighlightRulesSection';
import type {
    AdaptiveRendererMode,
    CursorStyle,
    FontFamily,
    RendererType,
    TerminalEncoding,
    TerminalSettings,
} from '@/store/settingsStore';

type TerminalTabProps = {
    terminal: TerminalSettings;
    updateTerminal: <K extends keyof TerminalSettings>(key: K, value: TerminalSettings[K]) => void;
};

export const TerminalTab = ({ terminal, updateTerminal }: TerminalTabProps) => {
    const { t } = useTranslation();
    const parseIntegerInput = (value: string, fallback: number) => {
        const parsed = parseInt(value, 10);
        return Number.isFinite(parsed) ? parsed : fallback;
    };

    const updateInBandTransfer = <K extends keyof TerminalSettings['inBandTransfer']>(
        key: K,
        value: TerminalSettings['inBandTransfer'][K],
    ) => {
        updateTerminal('inBandTransfer', {
            ...terminal.inBandTransfer,
            [key]: value,
        });
    };

    const updateAutosuggest = <K extends keyof TerminalSettings['autosuggest']>(
        key: K,
        value: TerminalSettings['autosuggest'][K],
    ) => {
        updateTerminal('autosuggest', {
            ...terminal.autosuggest,
            [key]: value,
        });
    };

    const updateCommandBar = <K extends keyof TerminalSettings['commandBar']>(
        key: K,
        value: TerminalSettings['commandBar'][K],
    ) => {
        updateTerminal('commandBar', {
            ...terminal.commandBar,
            [key]: value,
        });
    };

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-300">
            <div>
                <h3 className="text-2xl font-medium text-theme-text-heading mb-2">{t('settings_view.terminal.title')}</h3>
                <p className="text-theme-text-muted">{t('settings_view.terminal.description')}</p>
            </div>
            <Separator />

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.font')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.font_family')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.font_family_hint')}</p>
                        </div>
                        <Select value={terminal.fontFamily} onValueChange={(value) => updateTerminal('fontFamily', value as FontFamily)}>
                            <SelectTrigger className="w-[200px]">
                                <SelectValue placeholder={t('settings_view.terminal.select_font')} />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="jetbrains">JetBrains Mono NF (Subset) ✓</SelectItem>
                                <SelectItem value="meslo">MesloLGM NF (Subset) ✓</SelectItem>
                                <SelectItem value="maple">Maple Mono NF CN (Subset) ✓</SelectItem>
                                <SelectItem value="cascadia">Cascadia Code</SelectItem>
                                <SelectItem value="consolas">Consolas</SelectItem>
                                <SelectItem value="menlo">Menlo</SelectItem>
                                <SelectItem value="custom">{t('settings_view.terminal.custom_font')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    {terminal.fontFamily === 'custom' && (
                        <div className="flex items-center justify-between">
                            <div>
                                <Label className="text-theme-text">{t('settings_view.terminal.custom_font_stack')}</Label>
                                <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.custom_font_stack_hint')}</p>
                            </div>
                            <Input
                                type="text"
                                value={terminal.customFontFamily}
                                onChange={(event) => updateTerminal('customFontFamily', event.target.value)}
                                placeholder="'Sarasa Fixed SC', 'Fira Code', monospace"
                                className="w-[300px] font-mono text-sm"
                            />
                        </div>
                    )}

                    <div className="rounded-md border border-theme-border bg-theme-bg-sunken p-4">
                        <p className="text-xs text-theme-text-muted mb-2">{t('settings_view.terminal.font_preview')}</p>
                        <div
                            className="text-theme-text leading-relaxed"
                            style={{
                                fontFamily: getFontFamily(terminal.fontFamily, terminal.customFontFamily),
                                fontSize: `${terminal.fontSize}px`,
                                lineHeight: terminal.lineHeight,
                            }}
                        >
                            <div>ABCDEFG abcdefg 0123456789</div>
                            <div>Thực thi lệnh chậm - lưu, tổ chức, chạy</div>
                            <div className="text-theme-text-muted">{'-> => == != <= >= {}'}</div>
                            <div className="text-emerald-400">天地玄黄 The quick brown fox</div>
                            <div className="text-amber-400" style={{ letterSpacing: '0.1em' }}>       󰊤  </div>
                        </div>
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.font_size')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.font_size_hint')}</p>
                        </div>
                        <div className="flex items-center gap-3">
                            <Slider min={8} max={32} step={1} value={terminal.fontSize} onChange={(value) => updateTerminal('fontSize', value)} className="w-32" />
                            <div className="flex items-center gap-1">
                                <Input
                                    type="number"
                                    value={terminal.fontSize}
                                    onChange={(event) => updateTerminal('fontSize', parseInt(event.target.value, 10))}
                                    className="w-16"
                                />
                                <span className="text-xs text-theme-text-muted">px</span>
                            </div>
                        </div>
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.line_height')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.line_height_hint')}</p>
                        </div>
                        <Input
                            type="number"
                            step="0.1"
                            min="0.8"
                            max="3"
                            value={terminal.lineHeight}
                            onChange={(event) => updateTerminal('lineHeight', parseFloat(event.target.value))}
                            className="w-20"
                        />
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.encoding')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.encoding_hint')}</p>
                        </div>
                        <Select value={terminal.terminalEncoding ?? 'utf-8'} onValueChange={(value) => updateTerminal('terminalEncoding', value as TerminalEncoding)}>
                            <SelectTrigger className="w-[200px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="utf-8">UTF-8</SelectItem>
                                <SelectItem value="gbk">GBK</SelectItem>
                                <SelectItem value="gb18030">GB18030</SelectItem>
                                <SelectItem value="big5">Big5</SelectItem>
                                <SelectItem value="shift_jis">Shift_JIS</SelectItem>
                                <SelectItem value="euc-jp">EUC-JP</SelectItem>
                                <SelectItem value="euc-kr">EUC-KR</SelectItem>
                                <SelectItem value="windows-1252">Windows-1252</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.renderer')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.renderer_hint')}</p>
                        </div>
                        <Select value={terminal.renderer} onValueChange={(value) => updateTerminal('renderer', value as RendererType)}>
                            <SelectTrigger className="w-[200px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="auto">{t('settings_view.terminal.renderer_auto')}</SelectItem>
                                <SelectItem value="webgl">WebGL</SelectItem>
                                <SelectItem value="canvas">Canvas</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.adaptive_renderer')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.adaptive_renderer_hint')}</p>
                        </div>
                        <Select value={terminal.adaptiveRenderer ?? 'auto'} onValueChange={(value) => updateTerminal('adaptiveRenderer', value as AdaptiveRendererMode)}>
                            <SelectTrigger className="w-[200px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="auto">{t('settings_view.terminal.adaptive_renderer_auto')}</SelectItem>
                                <SelectItem value="always-60">{t('settings_view.terminal.adaptive_renderer_always60')}</SelectItem>
                                <SelectItem value="off">{t('settings_view.terminal.adaptive_renderer_off')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.show_fps_overlay')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.show_fps_overlay_hint')}</p>
                        </div>
                        <Checkbox
                            id="show-fps-overlay"
                            checked={terminal.showFpsOverlay ?? false}
                            onCheckedChange={(checked) => updateTerminal('showFpsOverlay', checked as boolean)}
                        />
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.cursor')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.cursor_style')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.cursor_style_hint')}</p>
                        </div>
                        <Select value={terminal.cursorStyle} onValueChange={(value) => updateTerminal('cursorStyle', value as CursorStyle)}>
                            <SelectTrigger className="w-[160px]">
                                <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="block">{t('settings_view.terminal.cursor_block')}</SelectItem>
                                <SelectItem value="underline">{t('settings_view.terminal.cursor_underline')}</SelectItem>
                                <SelectItem value="bar">{t('settings_view.terminal.cursor_bar')}</SelectItem>
                            </SelectContent>
                        </Select>
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.cursor_blink')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.cursor_blink_hint')}</p>
                        </div>
                        <Checkbox id="blink" checked={terminal.cursorBlink} onCheckedChange={(checked) => updateTerminal('cursorBlink', checked as boolean)} />
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.input_safety')}</h4>
                <div className="flex items-center justify-between">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.paste_protection')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.paste_protection_hint')}</p>
                    </div>
                    <Checkbox id="paste-protection" checked={terminal.pasteProtection} onCheckedChange={(checked) => updateTerminal('pasteProtection', checked as boolean)} />
                </div>
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.osc52_clipboard')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.osc52_clipboard_hint')}</p>
                    </div>
                    <Checkbox id="osc52-clipboard" checked={terminal.osc52Clipboard} onCheckedChange={(checked) => updateTerminal('osc52Clipboard', checked as boolean)} />
                </div>
                {!platform.isMac && (
                    <div className="flex items-center justify-between mt-4">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.smart_copy')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.smart_copy_hint')}</p>
                        </div>
                        <Checkbox id="smart-copy" checked={terminal.smartCopy} onCheckedChange={(checked) => updateTerminal('smartCopy', checked as boolean)} />
                    </div>
                )}
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.copy_on_select')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.copy_on_select_hint')}</p>
                    </div>
                    <Checkbox id="copy-on-select" checked={terminal.copyOnSelect} onCheckedChange={(checked) => updateTerminal('copyOnSelect', checked as boolean)} />
                </div>
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.middle_click_paste')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.middle_click_paste_hint')}</p>
                    </div>
                    <Checkbox id="middle-click-paste" checked={terminal.middleClickPaste} onCheckedChange={(checked) => updateTerminal('middleClickPaste', checked as boolean)} />
                </div>
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.selection_requires_shift')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.selection_requires_shift_hint')}</p>
                    </div>
                    <Checkbox id="selection-requires-shift" checked={terminal.selectionRequiresShift} onCheckedChange={(checked) => updateTerminal('selectionRequiresShift', checked as boolean)} />
                </div>
                <Separator className="my-5 opacity-50" />
                <div className="flex items-center justify-between">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.autosuggest')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.autosuggest_hint')}</p>
                    </div>
                    <Checkbox
                        id="terminal-autosuggest"
                        checked={terminal.autosuggest.enabled}
                        onCheckedChange={(checked) => updateAutosuggest('enabled', checked as boolean)}
                    />
                </div>
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.autosuggest_local_history')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.autosuggest_local_history_hint')}</p>
                    </div>
                    <Checkbox
                        id="terminal-autosuggest-local-history"
                        checked={terminal.autosuggest.localShellHistory}
                        onCheckedChange={(checked) => updateAutosuggest('localShellHistory', checked as boolean)}
                    />
                </div>
                <Separator className="my-5 opacity-50" />
                <div className="flex items-center justify-between">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.command_bar')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.command_bar_hint')}</p>
                    </div>
                    <Checkbox
                        id="terminal-command-bar"
                        checked={terminal.commandBar.enabled}
                        onCheckedChange={(checked) => updateCommandBar('enabled', checked as boolean)}
                    />
                </div>
                <div className="flex items-center justify-between mt-4">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.command_bar_legacy_toolbar')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.command_bar_legacy_toolbar_hint')}</p>
                    </div>
                    <Checkbox
                        id="terminal-command-bar-legacy-toolbar"
                        checked={terminal.commandBar.showLegacyToolbar}
                        onCheckedChange={(checked) => updateCommandBar('showLegacyToolbar', checked as boolean)}
                    />
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.in_band_transfer.title')}</h4>
                <div className="space-y-5">
                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.in_band_transfer.enabled')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.in_band_transfer.enabled_hint')}</p>
                        </div>
                        <Checkbox
                            id="in-band-transfer-enabled"
                            checked={terminal.inBandTransfer.enabled}
                            onCheckedChange={(checked) => updateInBandTransfer('enabled', checked as boolean)}
                        />
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.in_band_transfer.allow_directory')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.in_band_transfer.allow_directory_hint')}</p>
                        </div>
                        <Checkbox
                            id="in-band-transfer-allow-directory"
                            checked={terminal.inBandTransfer.allowDirectory}
                            onCheckedChange={(checked) => updateInBandTransfer('allowDirectory', checked as boolean)}
                        />
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between gap-6">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.in_band_transfer.max_chunk_bytes')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.in_band_transfer.max_chunk_bytes_hint')}</p>
                        </div>
                        <Input
                            type="number"
                            min={1024}
                            step={1024}
                            value={terminal.inBandTransfer.maxChunkBytes}
                            onChange={(event) => updateInBandTransfer('maxChunkBytes', parseIntegerInput(event.target.value, terminal.inBandTransfer.maxChunkBytes))}
                            className="w-32"
                        />
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between gap-6">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.in_band_transfer.max_file_count')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.in_band_transfer.max_file_count_hint')}</p>
                        </div>
                        <Input
                            type="number"
                            min={1}
                            step={1}
                            value={terminal.inBandTransfer.maxFileCount}
                            onChange={(event) => updateInBandTransfer('maxFileCount', parseIntegerInput(event.target.value, terminal.inBandTransfer.maxFileCount))}
                            className="w-32"
                        />
                    </div>

                    <Separator className="opacity-50" />

                    <div className="flex items-center justify-between gap-6">
                        <div>
                            <Label className="text-theme-text">{t('settings_view.terminal.in_band_transfer.max_total_bytes')}</Label>
                            <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.in_band_transfer.max_total_bytes_hint')}</p>
                        </div>
                        <Input
                            type="number"
                            min={1024}
                            step={1024}
                            value={terminal.inBandTransfer.maxTotalBytes}
                            onChange={(event) => updateInBandTransfer('maxTotalBytes', parseIntegerInput(event.target.value, terminal.inBandTransfer.maxTotalBytes))}
                            className="w-40"
                        />
                    </div>

                    <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-theme-text-muted">
                        {t('settings_view.terminal.in_band_transfer.runtime_note')}
                    </div>
                </div>
            </div>

            <div className="rounded-lg border border-theme-border bg-theme-bg-card p-5">
                <h4 className="text-sm font-medium text-theme-text mb-4 uppercase tracking-wider">{t('settings_view.terminal.buffer')}</h4>
                <div className="flex items-center justify-between">
                    <div>
                        <Label className="text-theme-text">{t('settings_view.terminal.scrollback')}</Label>
                        <p className="text-xs text-theme-text-muted mt-0.5">{t('settings_view.terminal.scrollback_hint')}</p>
                    </div>
                    <Input
                        type="number"
                        value={terminal.scrollback}
                        onChange={(event) => updateTerminal('scrollback', parseInt(event.target.value, 10))}
                        min={500}
                        max={20000}
                        className="w-28"
                    />
                </div>
            </div>

            <TerminalHighlightRulesSection
                rules={terminal.highlightRules}
                updateRules={(rules) => updateTerminal('highlightRules', rules)}
            />
        </div>
    );
};
