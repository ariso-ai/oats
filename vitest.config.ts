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
      // Pin the zone so date assertions don't depend on the machine running
      // them. `isoDateStamp` formats in *local* time on purpose, so without
      // this a midday-UTC fixture rolls to the next day at UTC+12 and up
      // (New Zealand, Fiji) and the filename tests fail there but not here.
      env: { TZ: 'UTC' },
    },
  })
);
