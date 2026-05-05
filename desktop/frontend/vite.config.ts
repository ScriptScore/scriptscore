import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

const frontendRoot = decodeURIComponent(new URL('.', import.meta.url).pathname);

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
