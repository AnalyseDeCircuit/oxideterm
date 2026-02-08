import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// https://vite.dev/config/
export default defineConfig(async () => {
  const host = process.env.TAURI_DEV_HOST || undefined;
  
  return {
  plugins: [react()],

  // Path aliases
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Build optimization - code splitting
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          // Terminal rendering (largest dependency)
          'xterm': [
            '@xterm/xterm',
            '@xterm/addon-webgl',
            '@xterm/addon-fit',
            '@xterm/addon-search',
            '@xterm/addon-web-links',
          ],
          // React core
          'vendor': ['react', 'react-dom', 'zustand'],
          // UI components
          'radix-ui': [
            '@radix-ui/react-dialog',
            '@radix-ui/react-select',
            '@radix-ui/react-tabs',
            '@radix-ui/react-dropdown-menu',
            '@radix-ui/react-toast',
            '@radix-ui/react-checkbox',
            '@radix-ui/react-radio-group',
            '@radix-ui/react-tooltip',
            '@radix-ui/react-switch',
          ],
          // Heavy dependencies — isolated chunks (dynamically imported)
          'mermaid': ['mermaid'],
          'katex': ['katex'],
          'office': ['mammoth', 'xlsx'],
          'prism': ['prismjs'],
          // Icons (large due to plugin system requiring full icon registry)
          'lucide': ['lucide-react'],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  };
});
