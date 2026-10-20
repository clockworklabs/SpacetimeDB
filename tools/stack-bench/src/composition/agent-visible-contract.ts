import { applyCredentialAliases } from './credential-aliases.js';
const INTERFACE_SECTION = /<!-- interface:([a-z][a-z0-9-]*) -->([\s\S]*?)<!-- \/interface -->/g;
const INTERNAL_LANGUAGE = /\b(?:stack\s*bench|benchmark|harness|grader|graded|grading|scored|scoring|tests?|testing|evaluation|criterion|testids?|external client|run configuration)\b|data-testid/i;

export function assertAgentVisibleText(text: string): string {
  const disclosure = text.match(INTERNAL_LANGUAGE)?.[0];
  if (disclosure) {
    throw new Error(`agent-facing text contains internal language ${JSON.stringify(disclosure)}`);
  }
  return text;
}

function selectedInterfaceText(value: unknown, selected: string | null): string {
  const source = String(value ?? '');
  let sections = 0;
  const output = source.replace(INTERFACE_SECTION, (_section, kind, content: string) => {
    sections += 1;
    return kind === selected ? content : '';
  });
  if (/<!--\s*(?:interface:|\/interface)/.test(output)) {
    throw new Error('interface-scoped contract has invalid markers');
  }
  if (sections && !selected) {
    throw new Error('interface-scoped contract requires a selected application interface');
  }
  return output.replace(/\n{3,}/g, '\n\n');
}

// Agent-facing documents are authored as agent-facing text. Reject accidental
// benchmark disclosure instead of trying to rewrite prose at runtime.
export function agentVisibleContractText(
  value: unknown,
  credentialAliases: Readonly<Record<string, string>> = {},
  applicationInterface: string | null = null,
): string {
  const text = applyCredentialAliases(
    selectedInterfaceText(value, applicationInterface), credentialAliases);
  return assertAgentVisibleText(text);
}

// Contract tables declare stable interface names in their first column.
export function contractInterfaceNames(contractText: unknown): string[] {
  const matches = String(contractText ?? '').matchAll(
    /^\s*\|\s*`([a-z][a-z0-9]*(?:-[a-z0-9]+)+)`\s*\|/gm,
  );
  const ids = [...matches]
    .map((match) => match[1])
    .filter((id): id is string => id !== undefined);
  return [...new Set(ids)].sort();
}

// Recipe identity hashes the interface-neutral text; each stack's qualification
// hashes its own blocks. A new interface block therefore changes only its stack.
export function interfaceNeutralText(value: unknown): string {
  return String(value ?? '').replace(INTERFACE_SECTION, '').replace(/\n{3,}/g, '\n\n');
}

export function interfaceBlocksText(value: unknown, selected: string): string {
  return [...String(value ?? '').matchAll(INTERFACE_SECTION)]
    .filter(match => match[1] === selected).map(match => match[2]).join('\n');
}
