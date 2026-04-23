// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type LinuxWebviewProfile = 'accelerated' | 'safe';

const ATTR_NAME = 'data-linux-webview-profile';

export function applyLinuxWebviewProfile(profile: LinuxWebviewProfile | null): void {
  if (typeof document === 'undefined') return;

  if (profile) {
    document.documentElement.setAttribute(ATTR_NAME, profile);
  } else {
    document.documentElement.removeAttribute(ATTR_NAME);
  }
}

export function getLinuxWebviewProfile(): LinuxWebviewProfile | null {
  if (typeof document === 'undefined') return null;

  const value = document.documentElement.getAttribute(ATTR_NAME);
  return value === 'accelerated' || value === 'safe' ? value : null;
}

export function isLinuxSafeWebviewProfile(): boolean {
  return getLinuxWebviewProfile() === 'safe';
}

export function linuxBackdropBlurClass(className: string): string {
  return isLinuxSafeWebviewProfile() ? '' : className;
}
