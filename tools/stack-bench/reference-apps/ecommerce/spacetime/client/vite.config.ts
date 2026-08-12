import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const vitePort = Number(process.env.VITE_PORT) || 6473;

export default defineConfig({
  plugins: [react()],
  server: {
    port: vitePort,
    host: true,
  },
});
