// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { refreshOpenPluginTabTitles } from './lib/plugin/pluginI18nManager';

// 命名空间列表（每种语言对应 22 个 JSON 文件）
const NAMESPACES = [
  'common', 'sidebar', 'settings', 'connections', 'forwards', 'modals',
  'sessions', 'settings_view', 'sftp', 'terminal', 'topology', 'ai',
  'editor', 'ide', 'fileManager', 'profiler', 'sessionManager', 'plugin',
  'graphics', 'launcher', 'eventLog', 'agent',
] as const;

// Vite 动态 glob：按需加载每个语言的 JSON 文件
const localeModules = import.meta.glob<{ default: Record<string, unknown> }>(
  './locales/**/*.json',
);

// 已加载语言缓存，避免重复加载
const loadedLanguages = new Set<string>();

/**
 * 按需加载指定语言的全部命名空间资源并合并为一个 translation 对象
 * 注意：所有命名空间的顶层 key 必须唯一，否则后加载的会覆盖先加载的。
 * 此约束由 pnpm i18n:check 脚本保障。
 */
async function loadLanguageResources(lang: string): Promise<Record<string, unknown>> {
  const merged: Record<string, unknown> = {};
  const promises = NAMESPACES.map(async (ns) => {
    const path = `./locales/${lang}/${ns}.json`;
    const loader = localeModules[path];
    if (loader) {
      const mod = await loader();
      Object.assign(merged, mod.default);
    }
  });
  await Promise.all(promises);
  return merged;
}

// 获取初始语言：优先本地存储 -> 浏览器语言 -> 默认中文
const getInitialLanguage = () => {
  const saved = localStorage.getItem('app_lang');
  if (saved) return saved;
  
  const browser = navigator.language;
  if (browser.startsWith('en')) return 'en';
  if (browser.startsWith('fr')) return 'fr-FR';
  if (browser.startsWith('ja')) return 'ja';
  if (browser.startsWith('es')) return 'es-ES';
  if (browser.startsWith('pt')) return 'pt-BR';
  if (browser.startsWith('vi')) return 'vi';
  if (browser.startsWith('ko')) return 'ko';
  if (browser.startsWith('de')) return 'de';
  if (browser.startsWith('it')) return 'it';
  if (browser === 'zh-TW' || browser === 'zh-Hant') return 'zh-TW';
  
  return 'zh-CN';
};

const initialLanguage = getInitialLanguage();

/**
 * 初始化 i18n：仅预加载当前语言 + en 回退，其他语言切换时按需加载
 */
async function initI18n() {
  // 仅加载当前语言和英文回退（避免加载全部 11 种语言）
  const langsToLoad = initialLanguage === 'en'
    ? ['en']
    : [initialLanguage, 'en'];

  const resources: Record<string, { translation: Record<string, unknown> }> = {};
  await Promise.all(langsToLoad.map(async (lang) => {
    const translations = await loadLanguageResources(lang);
    resources[lang] = { translation: translations };
    loadedLanguages.add(lang);
  }));

  await i18n
    .use(initReactI18next)
    .init({
      resources,
      lng: initialLanguage,
      fallbackLng: 'en',
      partialBundledLanguages: true,

      // React handles caching/escaping
      interpolation: {
        escapeValue: false,
      },

      // 调试模式 (仅开发环境启用)
      debug: import.meta.env.DEV,

      // 反应式设置
      react: {
        useSuspense: false,
      },
    });
}

// 并发保护：仅最后一次 changeLanguage 调用生效
let changeLanguageVersion = 0;

/**
 * 切换语言（按需加载资源后切换）
 * 快速连续调用时，仅最后一次生效，避免语言状态不一致
 */
export async function changeLanguage(lang: string) {
  const version = ++changeLanguageVersion;
  if (!loadedLanguages.has(lang)) {
    const translations = await loadLanguageResources(lang);
    if (version !== changeLanguageVersion) return; // 已被更新的调用取代
    // Deep-merge app resources so already-loaded plugin translations under the
    // top-level `plugin` namespace are preserved when a new language is loaded.
    i18n.addResourceBundle(lang, 'translation', translations, true, true);
    loadedLanguages.add(lang);
  }
  if (version !== changeLanguageVersion) return;
  await i18n.changeLanguage(lang);
  if (version !== changeLanguageVersion) return;
  refreshOpenPluginTabTitles();
}

// 导出初始化 Promise，main.tsx 需等待此 Promise 完成后再渲染
export const i18nReady = initI18n();

export default i18n;
