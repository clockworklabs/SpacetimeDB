import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
import fs from 'fs'
import { fileURLToPath } from 'url'

const __dirname = fileURLToPath(new URL('.', import.meta.url))

export default defineConfig({
  plugins: [
    react(),
    {
      name: 'serve-llm-data',
      configureServer(server) {
        const dataDir = path.resolve(__dirname, '../../docs/llms')
        server.middlewares.use('/docs/llms', (req, res, next) => {
          const filePath = path.join(dataDir, req.url ?? '/')
          if (!filePath.startsWith(dataDir)) { next(); return }
          if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
            res.setHeader('Content-Type', 'application/json')
            fs.createReadStream(filePath).pipe(res)
          } else {
            next()
          }
        })
      },
    },
  ],
  base: './',
  server: {
    host: true,
    port: process.env.PORT ? parseInt(process.env.PORT) : undefined,
    watch: {
      // Required for HMR inside Docker on Windows (no inotify support)
      usePolling: true,
      interval: 300,
    },
  },
})
