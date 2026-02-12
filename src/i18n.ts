import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

// zh-CN 翻译文件
import zhCNCommon from './locales/zh-CN/common.json';
import zhCNSidebar from './locales/zh-CN/sidebar.json';
import zhCNSettings from './locales/zh-CN/settings.json';
import zhCNConnections from './locales/zh-CN/connections.json';
import zhCNForwards from './locales/zh-CN/forwards.json';
import zhCNModals from './locales/zh-CN/modals.json';
import zhCNSessions from './locales/zh-CN/sessions.json';
import zhCNSettingsView from './locales/zh-CN/settings_view.json';
import zhCNSftp from './locales/zh-CN/sftp.json';
import zhCNTerminal from './locales/zh-CN/terminal.json';
import zhCNTopology from './locales/zh-CN/topology.json';
import zhCNAi from './locales/zh-CN/ai.json';
import zhCNEditor from './locales/zh-CN/editor.json';
import zhCNIde from './locales/zh-CN/ide.json';
import zhCNFileManager from './locales/zh-CN/fileManager.json';
import zhCNProfiler from './locales/zh-CN/profiler.json';
import zhCNSessionManager from './locales/zh-CN/sessionManager.json';
import zhCNPlugin from './locales/zh-CN/plugin.json';
import zhCNGraphics from './locales/zh-CN/graphics.json';
import zhCNLauncher from './locales/zh-CN/launcher.json';

// en 翻译文件
import enCommon from './locales/en/common.json';
import enSidebar from './locales/en/sidebar.json';
import enSettings from './locales/en/settings.json';
import enConnections from './locales/en/connections.json';
import enForwards from './locales/en/forwards.json';
import enModals from './locales/en/modals.json';
import enSessions from './locales/en/sessions.json';
import enSettingsView from './locales/en/settings_view.json';
import enSftp from './locales/en/sftp.json';
import enTerminal from './locales/en/terminal.json';
import enTopology from './locales/en/topology.json';
import enAi from './locales/en/ai.json';
import enEditor from './locales/en/editor.json';
import enIde from './locales/en/ide.json';
import enFileManager from './locales/en/fileManager.json';
import enProfiler from './locales/en/profiler.json';
import enSessionManager from './locales/en/sessionManager.json';
import enPlugin from './locales/en/plugin.json';
import enGraphics from './locales/en/graphics.json';
import enLauncher from './locales/en/launcher.json';

// fr-FR 翻译文件
import frFRCommon from './locales/fr-FR/common.json';
import frFRSidebar from './locales/fr-FR/sidebar.json';
import frFRSettings from './locales/fr-FR/settings.json';
import frFRConnections from './locales/fr-FR/connections.json';
import frFRForwards from './locales/fr-FR/forwards.json';
import frFRModals from './locales/fr-FR/modals.json';
import frFRSessions from './locales/fr-FR/sessions.json';
import frFRSettingsView from './locales/fr-FR/settings_view.json';
import frFRSftp from './locales/fr-FR/sftp.json';
import frFRTerminal from './locales/fr-FR/terminal.json';
import frFRTopology from './locales/fr-FR/topology.json';
import frFRAi from './locales/fr-FR/ai.json';
import frFREditor from './locales/fr-FR/editor.json';
import frFRIde from './locales/fr-FR/ide.json';
import frFRFileManager from './locales/fr-FR/fileManager.json';
import frFRProfiler from './locales/fr-FR/profiler.json';
import frFRSessionManager from './locales/fr-FR/sessionManager.json';
import frFRPlugin from './locales/fr-FR/plugin.json';
import frFRGraphics from './locales/fr-FR/graphics.json';
import frFRLauncher from './locales/fr-FR/launcher.json';

// ja 翻译文件
import jaCommon from './locales/ja/common.json';
import jaSidebar from './locales/ja/sidebar.json';
import jaSettings from './locales/ja/settings.json';
import jaConnections from './locales/ja/connections.json';
import jaForwards from './locales/ja/forwards.json';
import jaModals from './locales/ja/modals.json';
import jaSessions from './locales/ja/sessions.json';
import jaSettingsView from './locales/ja/settings_view.json';
import jaSftp from './locales/ja/sftp.json';
import jaTerminal from './locales/ja/terminal.json';
import jaTopology from './locales/ja/topology.json';
import jaAi from './locales/ja/ai.json';
import jaEditor from './locales/ja/editor.json';
import jaIde from './locales/ja/ide.json';
import jaFileManager from './locales/ja/fileManager.json';
import jaProfiler from './locales/ja/profiler.json';
import jaSessionManager from './locales/ja/sessionManager.json';
import jaPlugin from './locales/ja/plugin.json';
import jaGraphics from './locales/ja/graphics.json';
import jaLauncher from './locales/ja/launcher.json';

// es-ES 翻译文件
import esESCommon from './locales/es-ES/common.json';
import esESSidebar from './locales/es-ES/sidebar.json';
import esESSettings from './locales/es-ES/settings.json';
import esESConnections from './locales/es-ES/connections.json';
import esESForwards from './locales/es-ES/forwards.json';
import esESModals from './locales/es-ES/modals.json';
import esESSessions from './locales/es-ES/sessions.json';
import esESSettingsView from './locales/es-ES/settings_view.json';
import esESSftp from './locales/es-ES/sftp.json';
import esESTerminal from './locales/es-ES/terminal.json';
import esESTopology from './locales/es-ES/topology.json';
import esESAi from './locales/es-ES/ai.json';
import esESEditor from './locales/es-ES/editor.json';
import esESIde from './locales/es-ES/ide.json';
import esESFileManager from './locales/es-ES/fileManager.json';
import esESProfiler from './locales/es-ES/profiler.json';
import esESSessionManager from './locales/es-ES/sessionManager.json';
import esESPlugin from './locales/es-ES/plugin.json';
import esESGraphics from './locales/es-ES/graphics.json';
import esESLauncher from './locales/es-ES/launcher.json';

// pt-BR 翻译文件
import ptBRCommon from './locales/pt-BR/common.json';
import ptBRSidebar from './locales/pt-BR/sidebar.json';
import ptBRSettings from './locales/pt-BR/settings.json';
import ptBRConnections from './locales/pt-BR/connections.json';
import ptBRForwards from './locales/pt-BR/forwards.json';
import ptBRModals from './locales/pt-BR/modals.json';
import ptBRSessions from './locales/pt-BR/sessions.json';
import ptBRSettingsView from './locales/pt-BR/settings_view.json';
import ptBRSftp from './locales/pt-BR/sftp.json';
import ptBRTerminal from './locales/pt-BR/terminal.json';
import ptBRTopology from './locales/pt-BR/topology.json';
import ptBRAi from './locales/pt-BR/ai.json';
import ptBREditor from './locales/pt-BR/editor.json';
import ptBRIde from './locales/pt-BR/ide.json';
import ptBRFileManager from './locales/pt-BR/fileManager.json';
import ptBRProfiler from './locales/pt-BR/profiler.json';
import ptBRSessionManager from './locales/pt-BR/sessionManager.json';
import ptBRPlugin from './locales/pt-BR/plugin.json';
import ptBRGraphics from './locales/pt-BR/graphics.json';
import ptBRLauncher from './locales/pt-BR/launcher.json';

// vi 翻译文件
import viCommon from './locales/vi/common.json';
import viSidebar from './locales/vi/sidebar.json';
import viSettings from './locales/vi/settings.json';
import viConnections from './locales/vi/connections.json';
import viForwards from './locales/vi/forwards.json';
import viModals from './locales/vi/modals.json';
import viSessions from './locales/vi/sessions.json';
import viSettingsView from './locales/vi/settings_view.json';
import viSftp from './locales/vi/sftp.json';
import viTerminal from './locales/vi/terminal.json';
import viEditor from './locales/vi/editor.json';
import viTopology from './locales/vi/topology.json';
import viAi from './locales/vi/ai.json';
import viIde from './locales/vi/ide.json';
import viFileManager from './locales/vi/fileManager.json';
import viProfiler from './locales/vi/profiler.json';
import viSessionManager from './locales/vi/sessionManager.json';
import viPlugin from './locales/vi/plugin.json';
import viGraphics from './locales/vi/graphics.json';
import viLauncher from './locales/vi/launcher.json';

// ko 翻译文件
import koCommon from './locales/ko/common.json';
import koSidebar from './locales/ko/sidebar.json';
import koSettings from './locales/ko/settings.json';
import koConnections from './locales/ko/connections.json';
import koForwards from './locales/ko/forwards.json';
import koModals from './locales/ko/modals.json';
import koSessions from './locales/ko/sessions.json';
import koSettingsView from './locales/ko/settings_view.json';
import koSftp from './locales/ko/sftp.json';
import koTerminal from './locales/ko/terminal.json';
import koTopology from './locales/ko/topology.json';
import koAi from './locales/ko/ai.json';
import koEditor from './locales/ko/editor.json';
import koIde from './locales/ko/ide.json';
import koFileManager from './locales/ko/fileManager.json';
import koProfiler from './locales/ko/profiler.json';
import koSessionManager from './locales/ko/sessionManager.json';
import koPlugin from './locales/ko/plugin.json';
import koGraphics from './locales/ko/graphics.json';
import koLauncher from './locales/ko/launcher.json';

// de 翻译文件
import deCommon from './locales/de/common.json';
import deSidebar from './locales/de/sidebar.json';
import deSettings from './locales/de/settings.json';
import deConnections from './locales/de/connections.json';
import deForwards from './locales/de/forwards.json';
import deModals from './locales/de/modals.json';
import deSessions from './locales/de/sessions.json';
import deSettingsView from './locales/de/settings_view.json';
import deSftp from './locales/de/sftp.json';
import deTerminal from './locales/de/terminal.json';
import deTopology from './locales/de/topology.json';
import deAi from './locales/de/ai.json';
import deEditor from './locales/de/editor.json';
import deIde from './locales/de/ide.json';
import deFileManager from './locales/de/fileManager.json';
import deProfiler from './locales/de/profiler.json';
import deSessionManager from './locales/de/sessionManager.json';
import dePlugin from './locales/de/plugin.json';
import deGraphics from './locales/de/graphics.json';
import deLauncher from './locales/de/launcher.json';

// it 翻译文件 (意大利语)
import itCommon from './locales/it/common.json';
import itSidebar from './locales/it/sidebar.json';
import itSettings from './locales/it/settings.json';
import itConnections from './locales/it/connections.json';
import itForwards from './locales/it/forwards.json';
import itModals from './locales/it/modals.json';
import itSessions from './locales/it/sessions.json';
import itSettingsView from './locales/it/settings_view.json';
import itSftp from './locales/it/sftp.json';
import itTerminal from './locales/it/terminal.json';
import itTopology from './locales/it/topology.json';
import itAi from './locales/it/ai.json';
import itEditor from './locales/it/editor.json';
import itIde from './locales/it/ide.json';
import itFileManager from './locales/it/fileManager.json';
import itProfiler from './locales/it/profiler.json';
import itSessionManager from './locales/it/sessionManager.json';
import itPlugin from './locales/it/plugin.json';
import itGraphics from './locales/it/graphics.json';
import itLauncher from './locales/it/launcher.json';

// zh-TW 翻译文件 (繁体中文)
import zhTWCommon from './locales/zh-TW/common.json';
import zhTWSidebar from './locales/zh-TW/sidebar.json';
import zhTWSettings from './locales/zh-TW/settings.json';
import zhTWConnections from './locales/zh-TW/connections.json';
import zhTWForwards from './locales/zh-TW/forwards.json';
import zhTWModals from './locales/zh-TW/modals.json';
import zhTWSessions from './locales/zh-TW/sessions.json';
import zhTWSettingsView from './locales/zh-TW/settings_view.json';
import zhTWSftp from './locales/zh-TW/sftp.json';
import zhTWTerminal from './locales/zh-TW/terminal.json';
import zhTWTopology from './locales/zh-TW/topology.json';
import zhTWAi from './locales/zh-TW/ai.json';
import zhTWEditor from './locales/zh-TW/editor.json';
import zhTWIde from './locales/zh-TW/ide.json';
import zhTWFileManager from './locales/zh-TW/fileManager.json';
import zhTWProfiler from './locales/zh-TW/profiler.json';
import zhTWSessionManager from './locales/zh-TW/sessionManager.json';
import zhTWPlugin from './locales/zh-TW/plugin.json';
import zhTWGraphics from './locales/zh-TW/graphics.json';
import zhTWLauncher from './locales/zh-TW/launcher.json';

// 合并翻译资源
const zhCN = {
  ...zhCNCommon,
  ...zhCNSidebar,
  ...zhCNSettings,
  ...zhCNConnections,
  ...zhCNForwards,
  ...zhCNModals,
  ...zhCNSessions,
  ...zhCNSettingsView,
  ...zhCNSftp,
  ...zhCNTerminal,
  ...zhCNTopology,
  ...zhCNAi,
  ...zhCNEditor,
  ...zhCNIde,
  ...zhCNFileManager,
  ...zhCNProfiler,
  ...zhCNSessionManager,
  ...zhCNPlugin,
  ...zhCNGraphics,
  ...zhCNLauncher,
};

const enUS = {
  ...enCommon,
  ...enSidebar,
  ...enSettings,
  ...enConnections,
  ...enForwards,
  ...enModals,
  ...enSessions,
  ...enSettingsView,
  ...enSftp,
  ...enTerminal,
  ...enTopology,
  ...enAi,
  ...enEditor,
  ...enIde,
  ...enFileManager,
  ...enProfiler,
  ...enSessionManager,
  ...enPlugin,
  ...enGraphics,
  ...enLauncher,
};

const frFR = {
  ...frFRCommon,
  ...frFRSidebar,
  ...frFRSettings,
  ...frFRConnections,
  ...frFRForwards,
  ...frFRModals,
  ...frFRSessions,
  ...frFRSettingsView,
  ...frFRSftp,
  ...frFRTerminal,
  ...frFRTopology,
  ...frFRAi,
  ...frFREditor,
  ...frFRIde,
  ...frFRFileManager,
  ...frFRProfiler,
  ...frFRSessionManager,
  ...frFRPlugin,
  ...frFRGraphics,
  ...frFRLauncher,
};

const ja = {
  ...jaCommon,
  ...jaSidebar,
  ...jaSettings,
  ...jaConnections,
  ...jaForwards,
  ...jaModals,
  ...jaSessions,
  ...jaSettingsView,
  ...jaSftp,
  ...jaTerminal,
  ...jaTopology,
  ...jaAi,
  ...jaEditor,
  ...jaIde,
  ...jaFileManager,
  ...jaProfiler,
  ...jaSessionManager,
  ...jaPlugin,
  ...jaGraphics,
  ...jaLauncher,
};

const esES = {
  ...esESCommon,
  ...esESSidebar,
  ...esESSettings,
  ...esESConnections,
  ...esESForwards,
  ...esESModals,
  ...esESSessions,
  ...esESSettingsView,
  ...esESSftp,
  ...esESTerminal,
  ...esESTopology,
  ...esESAi,
  ...esESEditor,
  ...esESIde,
  ...esESFileManager,
  ...esESProfiler,
  ...esESSessionManager,
  ...esESPlugin,
  ...esESGraphics,
  ...esESLauncher,
};

const ptBR = {
  ...ptBRCommon,
  ...ptBRSidebar,
  ...ptBRSettings,
  ...ptBRConnections,
  ...ptBRForwards,
  ...ptBRModals,
  ...ptBRSessions,
  ...ptBRSettingsView,
  ...ptBRSftp,
  ...ptBRTerminal,
  ...ptBRTopology,
  ...ptBRAi,
  ...ptBREditor,
  ...ptBRIde,
  ...ptBRFileManager,
  ...ptBRProfiler,
  ...ptBRSessionManager,
  ...ptBRPlugin,
  ...ptBRGraphics,
  ...ptBRLauncher,
};

const vi = {
  ...viCommon,
  ...viSidebar,
  ...viSettings,
  ...viConnections,
  ...viForwards,
  ...viModals,
  ...viSessions,
  ...viSettingsView,
  ...viSftp,
  ...viTerminal,
  ...viTopology,
  ...viAi,
  ...viEditor,
  ...viIde,
  ...viFileManager,
  ...viProfiler,
  ...viSessionManager,
  ...viPlugin,
  ...viGraphics,
  ...viLauncher,
};

const ko = {
  ...koCommon,
  ...koSidebar,
  ...koSettings,
  ...koConnections,
  ...koForwards,
  ...koModals,
  ...koSessions,
  ...koSettingsView,
  ...koSftp,
  ...koTerminal,
  ...koTopology,
  ...koAi,
  ...koEditor,
  ...koIde,
  ...koFileManager,
  ...koProfiler,
  ...koSessionManager,
  ...koPlugin,
  ...koGraphics,
  ...koLauncher,
};

const de = {
  ...deCommon,
  ...deSidebar,
  ...deSettings,
  ...deConnections,
  ...deForwards,
  ...deModals,
  ...deSessions,
  ...deSettingsView,
  ...deSftp,
  ...deTerminal,
  ...deTopology,
  ...deAi,
  ...deEditor,
  ...deIde,
  ...deFileManager,
  ...deProfiler,
  ...deSessionManager,
  ...dePlugin,
  ...deGraphics,
  ...deLauncher,
};

const it = {
  ...itCommon,
  ...itSidebar,
  ...itSettings,
  ...itConnections,
  ...itForwards,
  ...itModals,
  ...itSessions,
  ...itSettingsView,
  ...itSftp,
  ...itTerminal,
  ...itTopology,
  ...itAi,
  ...itEditor,
  ...itIde,
  ...itFileManager,
  ...itProfiler,
  ...itSessionManager,
  ...itPlugin,
  ...itGraphics,
  ...itLauncher,
};

const zhTW = {
  ...zhTWCommon,
  ...zhTWSidebar,
  ...zhTWSettings,
  ...zhTWConnections,
  ...zhTWForwards,
  ...zhTWModals,
  ...zhTWSessions,
  ...zhTWSettingsView,
  ...zhTWSftp,
  ...zhTWTerminal,
  ...zhTWTopology,
  ...zhTWAi,
  ...zhTWEditor,
  ...zhTWIde,
  ...zhTWFileManager,
  ...zhTWProfiler,
  ...zhTWSessionManager,
  ...zhTWPlugin,
  ...zhTWGraphics,
  ...zhTWLauncher,
};

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

i18n
  .use(initReactI18next)
  .init({
    resources: {
      'zh-CN': { translation: zhCN },
      'en': { translation: enUS },
      'fr-FR': { translation: frFR },
      'ja': { translation: ja },
      'es-ES': { translation: esES },
      'pt-BR': { translation: ptBR },
      'vi': { translation: vi },
      'ko': { translation: ko },
      'de': { translation: de },
      'it': { translation: it },
      'zh-TW': { translation: zhTW }
    },
    lng: getInitialLanguage(),
    fallbackLng: 'en',
    
    // React handles caching/escaping
    interpolation: {
      escapeValue: false 
    },

    // 调试模式 (仅开发环境启用)
    debug: import.meta.env.DEV,
    
    // 反应式设置
    react: {
      useSuspense: false // 静态资源不需要 suspense
    }
  });

export default i18n;
