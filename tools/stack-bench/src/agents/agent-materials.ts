import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const SKILL_ID = /^[a-z][a-z0-9-]*$/;

export type ReadAgentMaterial = (path: string, encoding: 'utf8') => string;

export interface ReadAgentSkillDocumentOptions {
  read?: ReadAgentMaterial;
}

// Prompt material is part of the experiment identity. Git may check text out
// with platform-specific line endings, so normalize it before use or hashing.
export function normalizePromptText(text: string): string {
  if (typeof text !== 'string') throw new Error('prompt material must be text');
  return text.replace(/\r\n?/g, '\n');
}

function validateSkills(skills: string[]): string[] {
  if (!Array.isArray(skills)
    || skills.some(skill => typeof skill !== 'string' || !SKILL_ID.test(skill))
    || new Set(skills).size !== skills.length) {
    throw new Error('agent skills are invalid');
  }
  return skills;
}

export function selectAgentSkills(defaults: string[], requested: string[] | null = null): string[] {
  validateSkills(defaults);
  validateSkills(requested ?? []);
  return [...(requested ?? defaults)];
}

export function agentSkillPaths(stackBenchRoot: string, skills: string[]): string[] {
  // Container workflow instructions belong to Stack Bench, not the public SDK skills.
  return validateSkills(skills).map(skill => ['spacetime-dev', 'spacetime-managed-dev'].includes(skill)
    ? join(stackBenchRoot, 'backends', 'workflows', `${skill}.md`)
    : join(stackBenchRoot, '..', '..', 'skills', skill, 'SKILL.md'));
}

export function readAgentSkillDocuments(
  stackBenchRoot: string,
  skills: string[],
  { read = (path, encoding) => readFileSync(path, encoding) }: ReadAgentSkillDocumentOptions = {},
): string {
  const strip = (markdown: string): string => markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  return agentSkillPaths(stackBenchRoot, skills)
    .map(path => strip(normalizePromptText(read(path, 'utf8'))))
    .join('\n\n---\n\n');
}
