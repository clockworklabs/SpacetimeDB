# SpacetimeDB File Templates

## Backend Templates

### backend/spacetimedb/package.json
```json
{
  "name": "chat-app-backend",
  "type": "module",
  "version": "1.0.0",
  "dependencies": {
    "spacetimedb": "^2.0.0"
  }
}
```

### backend/spacetimedb/tsconfig.json
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "./dist"
  },
  "include": ["src/**/*"]
}
```

### File Organization
```
src/schema.ts   -> All tables, indexes, export spacetimedb
src/index.ts    -> Import schema, define all reducers and lifecycle hooks
```

Why this structure? Avoids circular dependency issues between tables and reducers.

---

## Client Templates

### client/package.json
```json
{
  "name": "chat-app-client",
  "private": true,
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "kill-port": "npx kill-port 5173 2>nul || true",
    "dev": "npm run kill-port && vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "spacetimedb": "^2.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.3.18",
    "@types/react-dom": "^18.3.5",
    "@vitejs/plugin-react": "^4.3.4",
    "typescript": "^5.7.2",
    "vite": "^6.0.3"
  }
}
```

### client/vite.config.ts
```typescript
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,  // NEVER use 3000 — conflicts with SpacetimeDB
  },
});
```

### client/tsconfig.json
```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}
```

### client/index.html
```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Chat App</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

### client/src/config.ts
```typescript
export const MODULE_NAME = 'chat-app-TIMESTAMP';  // Replace TIMESTAMP with actual module name
export const SPACETIMEDB_URI = 'ws://localhost:3000';
```

---

## Port Configuration

| Service | Port | Notes |
|---------|------|-------|
| SpacetimeDB server | 3000 | WebSocket connections |
| Vite dev server | 5173 | React client |

**Never run Vite on port 3000** — it conflicts with SpacetimeDB.
