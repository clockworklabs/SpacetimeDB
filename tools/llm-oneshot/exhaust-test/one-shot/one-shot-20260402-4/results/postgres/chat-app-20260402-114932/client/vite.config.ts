import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5474,
    proxy: {
      '/api': 'http://localhost:3301',
      '/socket.io': {
        target: 'http://localhost:3301',
        ws: true,
      },
    },
  },
});
