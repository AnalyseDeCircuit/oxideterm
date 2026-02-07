import React from 'react'
import ReactDOM from 'react-dom/client'
import { create } from 'zustand'
import * as lucideReact from 'lucide-react'
import { pluginUIKit } from './lib/plugin/pluginUIKit'
import App from './App'
import './styles.css'
import './i18n' // Import i18n configuration
import { initializeSettings } from './store/settingsStore'

// Expose shared modules so plugins can externalize react/zustand/lucide-react
// and avoid dual-instance hooks crashes.
window.__OXIDE__ = {
  React,
  ReactDOM: { createRoot: ReactDOM.createRoot },
  zustand: { create },
  lucideReact,
  ui: pluginUIKit,
}

// Initialize settings (including theme) before rendering
// This loads from oxide-settings-v2, applies theme, and cleans up legacy keys
initializeSettings()

const root = ReactDOM.createRoot(document.getElementById('root')!)

root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)

// Cleanup on window close to prevent memory leaks
// NOTE: UI state (sidebar) is now automatically persisted by settingsStore
window.addEventListener('beforeunload', () => {
  root.unmount()
})
