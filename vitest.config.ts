import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config';

// Derive from vite.config.ts so defines like __APP_VERSION__ have a single
// source of truth (a standalone vitest config would otherwise replace, not
// inherit, the vite config).
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'node',
      include: ['src/**/*.test.ts'],
    },
  })
);
