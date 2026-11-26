import type { Config } from '@react-router/dev/config';

export default {
  // Render on the server, so the first page load already shows the data
  // fetched by the route loaders.
  ssr: true,
} satisfies Config;
