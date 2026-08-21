import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 6673,
    proxy: {
      "/api": {
        target: "http://localhost:6401",
        changeOrigin: true,
      },
      "/socket.io": {
        target: "http://localhost:6401",
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
