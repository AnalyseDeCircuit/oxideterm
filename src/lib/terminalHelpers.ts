// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type React from 'react';
import type { BackgroundFit } from '../store/settingsStore';

/**
 * Convert 6-digit hex (#RRGGBB) to rgba() string.
 * xterm.js only parses #hex and rgba() formats — CSS keywords like
 * 'transparent' are NOT recognised and silently fall back to opaque black.
 */
export function hexToRgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** Map BackgroundFit to CSS properties */
export function getBackgroundFitStyles(fit: BackgroundFit): React.CSSProperties {
  switch (fit) {
    case 'cover':
      return { objectFit: 'cover', width: '100%', height: '100%' };
    case 'contain':
      return { objectFit: 'contain', width: '100%', height: '100%' };
    case 'fill':
      return { objectFit: 'fill', width: '100%', height: '100%' };
    case 'tile':
      return {};
  }
}

export type WebglRendererInfo = {
  renderer: string | null;
  vendor: string | null;
  isSoftwareRenderer: boolean;
};

const SOFTWARE_WEBGL_RENDERER_RE =
  /\b(llvmpipe|softpipe|swiftshader|software rasterizer|software adapter|mesa offscreen|d3d12 warp|swr|lavapipe)\b/i;

let _webglRendererInfo: WebglRendererInfo | null | undefined;

function normalizeWebglString(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null;
}

function getWebglContext(): WebGLRenderingContext | null {
  const canvas = document.createElement('canvas');
  const contextOptions = { failIfMajorPerformanceCaveat: true };
  const gl = canvas.getContext('webgl', contextOptions)
    || canvas.getContext('experimental-webgl', contextOptions)
    || canvas.getContext('webgl')
    || canvas.getContext('experimental-webgl');

  return gl && typeof (gl as WebGLRenderingContext).getParameter === 'function'
    ? gl as WebGLRenderingContext
    : null;
}

export function isSoftwareWebglRenderer(renderer: string | null | undefined): boolean {
  return Boolean(renderer && SOFTWARE_WEBGL_RENDERER_RE.test(renderer));
}

export function detectChunkedMarker(
  previousTail: string,
  chunkText: string,
  marker: string,
): { matched: boolean; tail: string } {
  const combined = `${previousTail}${chunkText}`;
  if (combined.includes(marker)) {
    return { matched: true, tail: '' };
  }

  return {
    matched: false,
    tail: combined.slice(-(marker.length - 1)),
  };
}

export function getWebglRendererInfo(): WebglRendererInfo | null {
  if (_webglRendererInfo !== undefined) return _webglRendererInfo;

  try {
    const gl = getWebglContext();
    if (!gl) {
      _webglRendererInfo = null;
      return null;
    }

    const ext = gl.getExtension('WEBGL_debug_renderer_info');
    const renderer = normalizeWebglString(
      ext
        ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)
        : gl.getParameter(gl.RENDERER),
    );
    const vendor = normalizeWebglString(
      ext
        ? gl.getParameter(ext.UNMASKED_VENDOR_WEBGL)
        : gl.getParameter(gl.VENDOR),
    );

    _webglRendererInfo = {
      renderer,
      vendor,
      isSoftwareRenderer: isSoftwareWebglRenderer(renderer),
    };
    return _webglRendererInfo;
  } catch {
    _webglRendererInfo = null;
    return null;
  }
}

export function logWebglRendererInfo(prefix: string, rendererInfo: WebglRendererInfo | null): void {
  if (!rendererInfo) {
    console.info(`${prefix} WebGL renderer info unavailable`);
    return;
  }
  console.info(
    `${prefix} WebGL vendor=${rendererInfo.vendor ?? 'unknown'} renderer=${rendererInfo.renderer ?? 'unknown'} software=${rendererInfo.isSoftwareRenderer}`,
  );
}

/**
 * Detect if the GPU is low-end (integrated graphics).
 * Returns true if we should cap blur to ≤5px for performance.
 * Uses WEBGL_debug_renderer_info when available.
 */
let _gpuDetectionResult: boolean | null = null;
export function isLowEndGPU(): boolean {
  if (_gpuDetectionResult !== null) return _gpuDetectionResult;
  const rendererInfo = getWebglRendererInfo();
  if (rendererInfo?.renderer) {
    const low = /Intel|Mesa|SwiftShader|llvmpipe|Apple GPU/i.test(rendererInfo.renderer);
    _gpuDetectionResult = low;
    return low;
  }
  _gpuDetectionResult = false;
  return false;
}

/**
 * Force xterm's internal DOM elements to transparent background.
 * Must be called after `term.open()`, after renderer restore, and after
 * any `term.options.theme = ...` assignment — xterm re-renders the
 * viewport from the parsed theme color on all of these occasions.
 */
export function forceViewportTransparent(container: HTMLElement | null): void {
  if (!container) return;
  const viewport = container.querySelector('.xterm-viewport') as HTMLElement | null;
  if (viewport) viewport.style.backgroundColor = 'transparent';
  const xtermEl = container.querySelector('.xterm') as HTMLElement | null;
  if (xtermEl) xtermEl.style.backgroundColor = 'transparent';
}

/** Clear DOM-level transparency overrides so xterm reverts to theme-driven background. */
export function clearViewportTransparent(container: HTMLElement | null): void {
  if (!container) return;
  const viewport = container.querySelector('.xterm-viewport') as HTMLElement | null;
  if (viewport) viewport.style.backgroundColor = '';
  const xtermEl = container.querySelector('.xterm') as HTMLElement | null;
  if (xtermEl) xtermEl.style.backgroundColor = '';
}

function isElementEditable(element: Element): boolean {
  if (element instanceof HTMLInputElement) {
    return !element.disabled && element.type !== 'button' && element.type !== 'checkbox' && element.type !== 'radio';
  }
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) {
    return !element.hasAttribute('disabled');
  }
  if (element instanceof HTMLElement) {
    return element.isContentEditable || element.getAttribute('role') === 'textbox';
  }
  return false;
}

function isTerminalOwnedFocus(element: Element): boolean {
  return Boolean(element.closest('.xterm, .terminal-container, [data-terminal-container="true"]'));
}

export function shouldAutoFocusTerminal(
  container: HTMLElement | null,
  activeElement: Element | null = typeof document !== 'undefined' ? document.activeElement : null,
): boolean {
  if (!isTerminalContainerRenderable(container)) return false;
  if (!activeElement || activeElement === document.body) return true;
  if (container?.contains(activeElement)) return true;
  if (isTerminalOwnedFocus(activeElement)) return true;
  return !isElementEditable(activeElement);
}

export function shouldFocusTerminalFromClick(
  target: EventTarget | null,
  selection: Selection | null = typeof window !== 'undefined' ? window.getSelection() : null,
): boolean {
  const element = target instanceof Element ? target : null;
  if (element?.closest(
    'input, textarea, select, button, a, [contenteditable=""], [contenteditable="true"], [role="button"], [role="link"], [role="textbox"], [data-terminal-focus-ignore="true"]',
  )) {
    return false;
  }

  if (selection && !selection.isCollapsed && selection.toString().trim().length > 0) {
    return false;
  }

  return true;
}

export interface TerminalDimensions {
  cols: number;
  rows: number;
}

/**
 * Hidden tab panels collapse terminal containers to zero size. Fitting xterm in
 * that state would incorrectly shrink the backing PTY and corrupt prompt layout.
 */
export function isTerminalContainerRenderable(container: HTMLElement | null): boolean {
  if (!container || !container.isConnected) return false;
  const rect = container.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

/**
 * Use freshly measured dimensions when the terminal is visible. Otherwise keep
 * the last stable xterm size instead of accepting hidden-tab measurements.
 */
export function resolveTerminalDimensions(
  container: HTMLElement | null,
  terminal: TerminalDimensions | null,
  fitAddon: { proposeDimensions: () => TerminalDimensions | null | undefined } | null,
): TerminalDimensions | null {
  const proposed = isTerminalContainerRenderable(container)
    ? fitAddon?.proposeDimensions() ?? null
    : null;
  const candidate = proposed ?? terminal;
  if (!candidate) return null;
  const { cols, rows } = candidate;
  if (!Number.isFinite(cols) || !Number.isFinite(rows) || cols <= 0 || rows <= 0) {
    return null;
  }
  return { cols, rows };
}
