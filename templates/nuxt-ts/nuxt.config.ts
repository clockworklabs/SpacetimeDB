// GHSA-xfq5-w38q-pc2x: server islands can render unsanitized useHeadSafe input
// as HTML. This template uses neither server islands nor useHeadSafe.
// GHSA-jjm6-p2m9-9w4x: crafted javascript: URLs can execute through NuxtLink
// when pages is disabled. This template has no NuxtLink or user-supplied link URLs.
// GHSA-vjh5-4x84-xrqp: case-variant paths bypass server routeRules. We define no
// routeRules and do not rely on them for access control. Reassess if these change.
export default defineNuxtConfig({
  compatibilityDate: '2025-01-01',
  telemetry: false,
  devServer: {
    port: 5173,
  },
});
