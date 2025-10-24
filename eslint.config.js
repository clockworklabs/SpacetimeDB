import js from '@eslint/js';
import globals from 'globals';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';

import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';
const __dirname = dirname(fileURLToPath(import.meta.url));

export default tseslint.config(
  {
    ignores: ['**/dist/**', '**/build/**', '**/coverage/**'],
  },
  js.configs.recommended,
  {
    files: ['**/*.{js,cjs,mjs}'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: globals.node,
      parserOptions: {
        project: false,
        tsconfigRootDir: __dirname,
      },
    },
  },
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser, ...globals.node },
      parserOptions: {
        project: [
          './tsconfig.json',
          './crates/bindings-typescript/tsconfig.json',
          './crates/bindings-typescript/test-app/tsconfig.json',
          './crates/bindings-typescript/examples/basic-react/tsconfig.json',
          './crates/bindings-typescript/examples/empty/tsconfig.json',
          './crates/bindings-typescript/examples/quickstart-chat/tsconfig.json',
          './docs/tsconfig.json',
        ],
        projectService: true,
        tsconfigRootDir: __dirname,
      },
    },
    linterOptions: {
      reportUnusedDisableDirectives: "off",
    },
    plugins: {
      '@typescript-eslint': tseslint.plugin,
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-namespace': 'error',
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          "argsIgnorePattern": "^_",
          "varsIgnorePattern": "^_",
          "destructuredArrayIgnorePattern": "^_",
          "caughtErrorsIgnorePattern": "^_"
        }
      ],
      'no-restricted-syntax': [
        'error',
        { selector: 'TSEnumDeclaration', message: 'Do not use enums; stick to JS-compatible types.' },
        { selector: 'TSEnumDeclaration[const=true]', message: 'Do not use const enum; use unions or objects.' },
        { selector: 'Decorator', message: 'Do not use decorators.' },
      ],
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
      "eslint-comments/no-unused-disable": "off",
    },
  }
);