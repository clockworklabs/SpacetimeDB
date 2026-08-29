export function normalizePromptText(text: string): string;
export function readAgentSkillDocuments(repository: string, skills: string[],
  options?: { read?: (path: string, encoding: 'utf8') => string }): string;
