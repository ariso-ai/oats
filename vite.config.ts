import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const host = process.env.TAURI_DEV_HOST;

// Fail fast with a clear message — a missing/malformed package.json means the
// workspace is broken, so no fallback version is substituted.
function readPackageVersion(): string {
  const path = fileURLToPath(new URL('./package.json', import.meta.url));
  try {
    const pkg = JSON.parse(readFileSync(path, 'utf-8')) as { version?: unknown };
    if (typeof pkg.version !== 'string') {
      throw new Error('missing "version" field');
    }
    return pkg.version;
  } catch (err) {
    throw new Error(
      `Failed to read app version from ${path}: ${err instanceof Error ? err.message : String(err)}`
    );
  }
}

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(readPackageVersion()),
  },

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
});
