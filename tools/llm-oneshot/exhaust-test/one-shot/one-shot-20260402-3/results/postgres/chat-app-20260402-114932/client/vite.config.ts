import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5274,
    proxy: {
      '/api': 'http://localhost:3101',
      '/socket.io': {
        target: 'http://localhost:3101',
        ws: true,
      },
    },
  },
});
