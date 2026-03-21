// @ts-check
import node from '@astrojs/node';
import react from '@astrojs/react';
import { defineConfig } from 'astro/config';

export default defineConfig({
  output: 'server',
  adapter: node({
    mode: 'standalone',
  }),
  integrations: [react()],
});
