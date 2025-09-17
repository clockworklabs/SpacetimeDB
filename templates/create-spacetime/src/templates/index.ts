export type ServerLanguage = "rust" | "C#";

const CLIENT_REPO = "clockworklabs/spacetimedb-typescript-sdk/examples/quickstart-chat";

export const TEMPLATES = {
  rust: {
    name: "Rust",
    serverLanguage: "rust" as const,
    clientRepository: CLIENT_REPO,
    serverRepository: "clockworklabs/SpacetimeDB/modules/quickstart-chat",
  },
  csharp: {
    name: "C#",
    serverLanguage: "C#" as const,
    clientRepository: CLIENT_REPO,
    serverRepository: "clockworklabs/SpacetimeDB/sdks/csharp/examples~/quickstart-chat/server",
  },
} as const;

export type TemplateKey = keyof typeof TEMPLATES;
export type Template = (typeof TEMPLATES)[TemplateKey];

export const getTemplateChoices = () =>
  Object.entries(TEMPLATES).map(([key, template]) => ({
    name: template.name,
    value: key,
    short: template.serverLanguage.toUpperCase(),
  }));

export const getValidTemplateKeys = (): string[] => Object.keys(TEMPLATES);

export const isValidTemplate = (key: string): key is TemplateKey => key in TEMPLATES;

export const getTemplate = (key: string): Template | undefined => TEMPLATES[key as TemplateKey];
