import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import sonarjs from 'eslint-plugin-sonarjs';
import svelte from 'eslint-plugin-svelte';
import svelteConfig from './svelte.config.js';
import tseslint from 'typescript-eslint';

const sonarOverrides = {
  'sonarjs/cognitive-complexity': ['error', 15],
  'sonarjs/no-duplicate-string': ['warn', { threshold: 5 }],
  'sonarjs/no-identical-expressions': 'error',
  'sonarjs/no-collapsible-if': 'error',
  'sonarjs/no-nested-conditional': 'off',
  'sonarjs/publicly-writable-directories': 'off',
  'sonarjs/unused-import': 'off',
  'sonarjs/use-type-alias': 'off',
  'sonarjs/void-use': 'off'
};

export default defineConfig(
  {
    ignores: ['build/**', 'coverage/**', '.svelte-kit/**']
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  sonarjs.configs.recommended,
  {
    files: ['src/**/*.{js,ts,svelte}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node
      }
    },
    rules: sonarOverrides
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
    files: ['src/**/*.{test,spec}.{js,ts}', 'src/test/**/*.{js,ts}'],
    rules: {
      'sonarjs/no-duplicate-string': 'off'
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
