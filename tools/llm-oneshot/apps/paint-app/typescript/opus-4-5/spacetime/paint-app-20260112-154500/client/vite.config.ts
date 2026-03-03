import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173, // NEVER use 3000 — conflicts with SpacetimeDB
    host: '127.0.0.1', // Bind to IPv4 localhost
  },
});
