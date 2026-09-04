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

export function agentSkillPaths(repository: string, skills: string[]): string[] {
  return validateSkills(skills).map(skill => join(repository, 'skills', skill, 'SKILL.md'));
}

export function readAgentSkillDocuments(
  repository: string,
  skills: string[],
  { read = (path, encoding) => readFileSync(path, encoding) }: ReadAgentSkillDocumentOptions = {},
): string {
  const strip = (markdown: string): string => markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  return agentSkillPaths(repository, skills)
    .map(path => strip(normalizePromptText(read(path, 'utf8'))))
    .join('\n\n---\n\n');
}
