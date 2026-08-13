import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const SKILL_ID = /^[a-z][a-z0-9-]*$/;

function validateSkills(skills) {
  if (!Array.isArray(skills) || skills.some(skill => typeof skill !== 'string' || !SKILL_ID.test(skill))
    || new Set(skills).size !== skills.length) throw new Error('agent skills are invalid');
  return skills;
}

export function selectAgentSkills(defaults, requested = null) {
  validateSkills(defaults);
  validateSkills(requested ?? []);
  const selected = defaults.length ? (requested ?? defaults) : [];
  return [...selected];
}

export function agentSkillPaths(repository, skills) {
  return validateSkills(skills).map(skill => join(repository, 'skills', skill, 'SKILL.md'));
}

export function readAgentSkillDocuments(repository, skills, { read = readFileSync } = {}) {
  const strip = markdown => markdown.replace(/^---\n[\s\S]*?\n---\n/, '');
  return agentSkillPaths(repository, skills).map(path => strip(read(path, 'utf8'))).join('\n\n---\n\n');
}
