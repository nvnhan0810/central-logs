import { fileURLToPath, URL } from 'node:url';

import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    // Must match CENTRAL_LOGS_FRONTEND_URL (default http://localhost:3000), which is
    // also the origin the backend allows for CORS.
    port: 3000,
    strictPort: true,
  },
});
