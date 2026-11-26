/** @type {import("@sveltejs/vite-plugin-svelte").SvelteConfig} */
export default {
  vitePlugin: {
    // Compile this app's components in runes mode, so a component can't
    // silently fall back to legacy mode. Components from dependencies keep
    // whichever mode they were written in.
    dynamicCompileOptions: ({ filename }) =>
      filename.split(/[/\\]/).includes('node_modules')
        ? undefined
        : { runes: true },
  },
};
