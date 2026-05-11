import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import svelte from 'eslint-plugin-svelte';
import svelteConfig from './svelte.config.js';
import tseslint from 'typescript-eslint';

export default defineConfig(
  {
    ignores: ['build/**', 'coverage/**', '.svelte-kit/**']
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    files: ['src/**/*.{js,ts,svelte}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node
      }
    }
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        parser: tseslint.parser,
        svelteConfig
      }
    },
    rules: {
      'no-useless-assignment': 'off',
      'svelte/no-immutable-reactive-statements': 'off',
      'svelte/prefer-svelte-reactivity': 'off'
    }
  },
  {
    files: ['src/lib/components/desktop/SettingsWorkspace.svelte'],
    rules: {
      // Debounced Canvas LMS validation intentionally schedules async work from a reactive block; no loop.
      'svelte/infinite-reactive-loop': 'off'
    }
  }
);
