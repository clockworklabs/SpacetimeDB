import { applyCredentialAliases } from './credential-aliases.mjs';

// Contract sources keep internal grading terms. Coding agents receive only the
// public application requirements and stable interface names.
export function agentVisibleContractText(value, credentialAliases = {}) {
  return applyCredentialAliases(value, credentialAliases)
    .replace(/The app is graded by an automated harness that locates elements \*\*only\*\* via\s+/gi,
      'Expose ')
    .replace(/What the harness needs/gi, 'Run configuration')
    .replace(/public testing interface/gi, 'application interface')
    .replace(/testing interface/gi, 'application interface')
    .replace(/test interface/gi, 'application interface')
    .replace(/the test runner locates visible controls through exact `data-testid` attributes\./gi,
      'Use the following exact `data-testid` attributes on the corresponding visible controls.')
    .replace(/the runner locates visible controls through exact `data-testid` attributes\./gi,
      'Use the following exact `data-testid` attributes on the corresponding visible controls.')
    .replace(/the test runner also issues the same account writes as the UI\./gi,
      'Expose the same account writes used by the UI.')
    .replace(/the runner also issues the same account writes as the UI\./gi,
      'Expose the same account writes used by the UI.')
    .replace(/the test runner locates/gi, 'The application exposes')
    .replace(/the runner locates/gi, 'The application exposes')
    .replace(/the test runner also issues/gi, 'The application also exposes')
    .replace(/the runner also issues/gi, 'The application also exposes')
    .replace(/test runner/gi, 'external client')
    .replace(/the runner/gi, 'the external client')
    .replace(/test fixture/gi, 'run configuration')
    .replace(/test handle/gi, 'interface value')
    .replace(/direct authorization tests/gi, 'direct authorization actions')
    .replace(/testing call/gi, 'application action')
    .replace(/\btest action\b/gi, 'application action')
    .replace(/Do not add another transport\s+only for Stack Bench\./gi,
      'Use the same transport as the visible application.')
    .replace(/Test ID/gi, 'Element ID')
    .replace(/Test id/gi, 'Element ID')
    .replace(/\bhooks\b/gi, 'controls')
    .replace(/\bhook\b/gi, 'control')
    .replace(/`data-testid` attributes/gi, '`id` attributes')
    .replace(/data-testid/gi, 'id')
    .replace(/harness/gi, 'runtime')
    .replace(/benchmark/gi, 'runtime');
}

// Contract fragments use backticks for stable element IDs. Passwords
// and custom data attributes share the notation but are not element IDs.
export function contractControlIds(contractText) {
  return [...new Set([...String(contractText ?? '').matchAll(
    /`([a-z][a-z0-9]*(?:-[a-z0-9]+)+)`/g)].map(match => match[1])
    .filter(id => !id.startsWith('stackbench-') && !id.startsWith('data-')))].sort();
}
