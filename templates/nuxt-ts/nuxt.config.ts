// This template does not use server islands, useHeadSafe, NuxtLink,
// or routeRules for access control; issues requiring those paths do not apply.
// Revisit that assessment before adding those features.
export default defineNuxtConfig({
  compatibilityDate: '2025-01-01',
  telemetry: false,
  devServer: {
    port: 5173,
  },
});
