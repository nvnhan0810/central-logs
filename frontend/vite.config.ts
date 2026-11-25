import { fileURLToPath, URL } from 'node:url';

import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

// The Rust backend (backend/, default 127.0.0.1:8080) owns /api, including the
// session cookie. Proxying it keeps the browser on one origin, so the cookie is
// first-party and SameSite=Lax works without CORS.
const BACKEND_URL = process.env.VITE_BACKEND_URL ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    // Must match CENTRAL_LOGS_FRONTEND_URL (default http://localhost:3000).
    port: 3000,
    strictPort: true,
    proxy: {
      '/api': {
        target: BACKEND_URL,
        changeOrigin: false,
      },
    },
  },
});
