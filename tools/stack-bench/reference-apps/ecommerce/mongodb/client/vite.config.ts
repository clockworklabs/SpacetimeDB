import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const apiPort = Number(process.env.API_PORT) || 6401;
const vitePort = Number(process.env.VITE_PORT) || 6673;

export default defineConfig({
  plugins: [react()],
  server: {
    host: "0.0.0.0",
    port: vitePort,
    proxy: {
      "/api": {
        target: `http://localhost:${apiPort}`,
        changeOrigin: true,
      },
      "/socket.io": {
        target: `http://localhost:${apiPort}`,
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
