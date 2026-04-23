import { afterEach, describe, expect, it } from 'vitest';

import {
  applyLinuxWebviewProfile,
  getLinuxWebviewProfile,
  isLinuxSafeWebviewProfile,
  linuxBackdropBlurClass,
} from '@/lib/linuxWebviewProfile';

describe('linuxWebviewProfile', () => {
  afterEach(() => {
    applyLinuxWebviewProfile(null);
  });

  it('stores and reads the current profile from the document root', () => {
    applyLinuxWebviewProfile('safe');

    expect(document.documentElement.getAttribute('data-linux-webview-profile')).toBe('safe');
    expect(getLinuxWebviewProfile()).toBe('safe');
    expect(isLinuxSafeWebviewProfile()).toBe(true);
  });

  it('removes the attribute when no profile is set', () => {
    applyLinuxWebviewProfile('accelerated');
    applyLinuxWebviewProfile(null);

    expect(document.documentElement.hasAttribute('data-linux-webview-profile')).toBe(false);
    expect(getLinuxWebviewProfile()).toBeNull();
  });

  it('drops blur classes only in safe profile', () => {
    applyLinuxWebviewProfile('accelerated');
    expect(linuxBackdropBlurClass('backdrop-blur-sm')).toBe('backdrop-blur-sm');

    applyLinuxWebviewProfile('safe');
    expect(linuxBackdropBlurClass('backdrop-blur-sm')).toBe('');
  });
});
