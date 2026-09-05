import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

const frontendRoot = decodeURIComponent(new URL('.', import.meta.url).pathname);
const disableNodeWebStorageFlag = '--no-experimental-webstorage';
const supportsDisableNodeWebStorage =
  (
    globalThis as typeof globalThis & {
      process?: { allowedNodeEnvironmentFlags?: { has(flag: string): boolean } };
    }
  ).process?.allowedNodeEnvironmentFlags?.has(disableNodeWebStorageFlag) ?? false;

function distroboxPathAliases(path: string): string[] {
  if (path.startsWith('/var/home/')) {
    return [path.replace('/var/home/', '/home/')];
  }

  if (path.startsWith('/home/')) {
    return [path.replace('/home/', '/var/home/')];
  }

  return [];
}

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  clearScreen: false,
  resolve: {
    conditions: ['browser']
  },
  server: {
    port: 5173,
    strictPort: true,
    fs: {
      allow: [frontendRoot, ...distroboxPathAliases(frontendRoot)]
    }
  },
  test: {
    environment: 'jsdom',
    // Node 25+ exposes Web Storage globals that shadow jsdom's per-test storage.
    execArgv: supportsDisableNodeWebStorage ? [disableNodeWebStorageFlag] : [],
    setupFiles: ['src/test/setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,js}'],
    clearMocks: true,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json-summary', 'lcov'],
      reportsDirectory: './coverage',
      include: ['src/lib/**/*.{ts,js,svelte}', 'src/routes/**/*.{ts,js,svelte}'],
      exclude: ['src/**/*.d.ts', 'src/**/*.test.{ts,js}', 'src/test/**']
    }
  }
});
