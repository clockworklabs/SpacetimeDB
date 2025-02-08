# SpacetimeDB TypeScript Quickstart Chat

This is a simple chat application that demonstrates how to use SpacetimeDB with TypeScript and React. The chat application is a simple chat room where users can send messages to each other. The chat application uses SpacetimeDB to store the chat messages.

It is based directly on the plain React + TypeScript + Vite template. You can follow the quickstart guide for how creating this project from scratch at [SpacetimeDB TypeScript Quickstart](https://spacetimedb.com/docs/sdks/typescript/quickstart).

You can follow the instructions for creating your own SpacetimeDB module here: [SpacetimeDB Rust Module Quickstart](https://spacetimedb.com/docs/modules/rust/quickstart). Place the module in the `quickstart-chat/server` directory for compability with this project.

In order to run this example, you need to:

- `pnpm compile` in the root directory (`spacetimedb-typescriptsdk`)
- `pnpm install` in this directory
- `pnpm run build` in this directory
- `pnpm run dev` in this directory to run the example

Below is copied from the original template README:

# React + TypeScript + Vite

This template provides a minimal setup to get React working in Vite with HMR and some ESLint rules.

Currently, two official plugins are available:

- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react/README.md) uses [Babel](https://babeljs.io/) for Fast Refresh
- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react-swc) uses [SWC](https://swc.rs/) for Fast Refresh

## Expanding the ESLint configuration

If you are developing a production application, we recommend updating the configuration to enable type aware lint rules:

- Configure the top-level `parserOptions` property like this:

```js
export default tseslint.config({
  languageOptions: {
    // other options...
    parserOptions: {
      project: ['./tsconfig.node.json', './tsconfig.app.json'],
      tsconfigRootDir: import.meta.dirname,
    },
  },
});
```

- Replace `tseslint.configs.recommended` to `tseslint.configs.recommendedTypeChecked` or `tseslint.configs.strictTypeChecked`
- Optionally add `...tseslint.configs.stylisticTypeChecked`
- Install [eslint-plugin-react](https://github.com/jsx-eslint/eslint-plugin-react) and update the config:

```js
// eslint.config.js
import react from 'eslint-plugin-react';

export default tseslint.config({
  // Set the react version
  settings: { react: { version: '18.3' } },
  plugins: {
    // Add the react plugin
    react,
  },
  rules: {
    // other rules...
    // Enable its recommended rules
    ...react.configs.recommended.rules,
    ...react.configs['jsx-runtime'].rules,
  },
});
```
