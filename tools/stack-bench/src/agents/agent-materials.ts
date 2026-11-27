import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { assertAgentVisibleText } from '../composition/agent-visible-contract.js';

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

// StackBench's own workflow pages describe its sandbox, so they go in the prompt; product skills are installed.
const WORKFLOW_SKILLS = new Set(['spacetime-dev']);
export const isWorkflowSkill = (id: string): boolean => WORKFLOW_SKILLS.has(id);

export function agentSkillPaths(stackBenchRoot: string, skills: string[]): string[] {
  return validateSkills(skills).map(skill => isWorkflowSkill(skill)
    ? join(stackBenchRoot, 'backends', 'workflows', `${skill}.md`)
    : join(stackBenchRoot, '..', '..', 'skills', skill, 'SKILL.md'));
}

// The skill files the agent's CLI installs, frontmatter included, so it can list them by name and description.
export function readAgentSkillFiles(
  stackBenchRoot: string,
  skills: string[],
  { read = (path, encoding) => readFileSync(path, encoding) }: ReadAgentSkillDocumentOptions = {},
): { id: string; text: string }[] {
  const paths = agentSkillPaths(stackBenchRoot, skills);
  return skills.map((id, index) => ({ id, text: normalizePromptText(read(paths[index]!, 'utf8')) }));
}

export function readAgentSkillDocuments(
  stackBenchRoot: string,
  skills: string[],
  { read = (path, encoding) => readFileSync(path, encoding) }: ReadAgentSkillDocumentOptions = {},
): string {
  const strip = (markdown: string): string => markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  const workflows = join(stackBenchRoot, 'backends', 'workflows');
  return agentSkillPaths(stackBenchRoot, skills).map(path => {
    const text = strip(normalizePromptText(read(path, 'utf8')));
    // The prompt check skips the product's own skills; StackBench's workflow documents still get it.
    return path.startsWith(workflows) ? assertAgentVisibleText(text) : text;
  }).join('\n\n---\n\n');
}
