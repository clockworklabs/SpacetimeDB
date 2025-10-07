const CLIENT_REPO = "clockworklabs/SpacetimeDB/crates/bindings-typescript/examples/quickstart-chat";

export const TEMPLATES = {
  rust: {
    name: "Rust",
    serverLanguage: "rust",
    serverRepository: "clockworklabs/SpacetimeDB/modules/quickstart-chat",
  },
  csharp: {
    name: "C#",
    serverLanguage: "C#",
    serverRepository: "clockworklabs/SpacetimeDB/sdks/csharp/examples~/quickstart-chat/server",
  },
} as const;

export type TemplateKey = keyof typeof TEMPLATES;

export const DEFAULT_TEMPLATE: TemplateKey = "rust";

export const getTemplateChoices = () => [
  ...Object.entries(TEMPLATES).map(([key, config]) => ({
    name: config.name,
    value: key,
    short: config.serverLanguage,
  })),
  {
    name: "Other (GitHub)",
    value: "custom",
    short: "Other",
  },
];

export const isValidTemplate = (key: string): key is TemplateKey => key in TEMPLATES;

export const getTemplate = (key: string) =>
  isValidTemplate(key) ? { ...TEMPLATES[key], clientRepository: CLIENT_REPO } : undefined;
