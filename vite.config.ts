import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true, watch: { ignored: ['**/.tools/**', '**/target/**', '**/.implementation/**', '**/src-tauri/**', '**/browser-extension/dist/**'] } },
  test: { environment: 'jsdom', environmentOptions: { jsdom: { url: 'https://example.test/login' } }, include: ['tests/**/*.test.ts'] },
});
