import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import { defineConfig } from 'eslint/config';

export default defineConfig([
  { ignores: ['dist'] },
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser, ...globals.node },
    },
    plugins: {
      '@typescript-eslint': tseslint.plugin,
    },
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-namespace': 'error',
      'no-restricted-syntax': [
        'error',
        {
          selector: 'TSEnumDeclaration',
          message: 'Do not use enums; stick to JS-compatible types.',
        },
        {
          selector: 'TSEnumDeclaration[const=true]',
          message: 'Do not use const enum; use unions or objects.',
        },
        { selector: 'Decorator', message: 'Do not use decorators.' },
        {
          selector: 'TSParameterProperty',
          message: 'Do not use parameter properties.',
        },
      ],
    },
  },
]);
