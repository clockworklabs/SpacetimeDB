// Findings: the closed catalog of ways an action can fail. An executor never
// fails with a sentence; it fails with a kind and its fields, and every reader
// renders the finding from one template. Fields are things the coding agent
// already has or the application produced: contract control names, action
// ids, actor labels, numbers, counts, HTTP statuses. A scenario's own probe
// text is never a field. `detail` is for a human reading the artifact and is
// never rendered.

import { z } from 'zod';

interface Control { readonly control: string }
interface Actor { readonly actor: string }
interface Actors { readonly actors: readonly string[] }
interface Action { readonly action: string }
interface Status { readonly status: number | null }
interface Detail { readonly detail?: string }

export interface NumberExpectation {
  readonly equals?: number;
  readonly atLeast?: number;
  readonly atMost?: number;
}

export interface Operation {
  readonly reducer: string | null;
  readonly path: string | null;
  readonly method: string;
}

export type LifecycleTarget = 'app-server' | 'backend-runtime';

// The application did not do what the criterion requires.
export interface FailedFindingFields {
  'control-missing': Control;
  'control-present': Control;
  'control-available': Control & Actor;
  'control-not-ready': Control & Actors;
  'control-blocked': Partial<Control> & Detail;
  'control-empty': Control;
  'control-unreadable': Control & Actors;
  'value-mismatch': Control;
  'text-unexpected': Control;
  'value-unstable': Control;
  'clients-disagree': Control & Actors;
  'number-missing': Control;
  'number-mismatch': Control & { readonly observed: number | null; readonly expected: NumberExpectation };
  'count-mismatch': Control & { readonly observed: number; readonly expected: number };
  'order-mismatch': Control & Partial<Actors>;
  'entries-missing': { readonly expected: number; readonly missing: number; readonly duplicated: number };
  'actors-with-control': Control & { readonly observed: number; readonly expected: number };
  'too-many-per-actor': Control & { readonly maxEach: number };
  'clicks-failed': Control & { readonly failed: number; readonly total: number } & Detail;
  'choice-missing': Partial<Control> & Detail;
  'page-timeout': Partial<Control> & Detail;
  'page-crashed': Detail;
  'page-error': Partial<Control> & Detail;
  'app-control-failed': { readonly mode: string; readonly target: LifecycleTarget } & Detail;
  'script-failed': { readonly script: string } & Detail;
  'script-invalid': { readonly script: string };
  'action-failed': Action;
  'call-refused': Action & Actor & Status & { readonly operation: Operation | null };
  'call-accepted': Action & Actor & Status & { readonly required: 'refused' | 'validation-refused' };
  'call-error': Action & Actor & Status & { readonly required: 'refused' | 'validation-refused';
    readonly operation: Operation | null };
  'concurrent-calls-mismatch': Action & { readonly expected: number; readonly accepted: number;
    readonly fired: number } & Detail;
  'interface-missing': Control & Action & { readonly attribute: string };
  'interface-invalid': Action & { readonly attribute: string; readonly missing?: readonly string[];
    readonly unexpected?: readonly string[] } & Detail;
  'replay-accepted': Actor & Status & Partial<Action>;
  'replay-error': Status & Partial<Action>;
  'forgery-accepted': Status & { readonly field: string };
  'forgery-error': Status;
  'message-delivered': Actor;
}

// The harness could not measure. Never repair feedback.
export interface InconclusiveFindingFields {
  'assertion-without-action': Action;
  'unknown-action': Action;
  'action-without-parameters': Action;
  'no-session': Actor & Partial<Action>;
  'unresolved-action': Partial<Action>;
  'replay-unavailable': Actor & Detail;
  'forgery-unverifiable': Actor & Detail;
  'not-observed': Actor;
  'nothing-contended': Detail;
  'no-backend-control': { readonly target: LifecycleTarget };
  'control-refused': { readonly target: LifecycleTarget };
  'database-write-failed': Detail;
  'unsupported-backend': { readonly backend: string };
  'app-directory-unknown': Record<string, never>;
  'invalid-input': Detail;
}

export type FailedFindingKind = keyof FailedFindingFields;
export type InconclusiveFindingKind = keyof InconclusiveFindingFields;
export type FindingFields = FailedFindingFields & InconclusiveFindingFields;
export type FindingKind = FailedFindingKind | InconclusiveFindingKind;
export type FindingStatus = 'failed' | 'inconclusive';

export type Finding = { [K in FindingKind]: { readonly kind: K; readonly fields: FindingFields[K] } }[FindingKind];

type Renderers<Fields> = { readonly [K in keyof Fields]: (fields: Fields[K]) => string };

const control = (name: string): string => `the ${name} control`;
const names = (values: readonly string[]): string => values.join(', ');
const http = (status: number | null): string => status ? `HTTP ${status}` : 'no server response';
const operation = (value: Operation | null): string => {
  if (!value) return '';
  const parts = [value.reducer ? `the ${value.reducer} reducer` : null,
    value.path ? `${value.method} ${value.path}` : null].filter(Boolean);
  return parts.length ? `; the application interface names ${parts.join(' or ')}` : '';
};
const expectation = (value: NumberExpectation): string => [
  value.equals === undefined ? null : `exactly ${value.equals}`,
  value.atLeast === undefined ? null : `at least ${value.atLeast}`,
  value.atMost === undefined ? null : `at most ${value.atMost}`,
].filter(Boolean).join(' and ') || 'a number';
const target = (value: LifecycleTarget): string =>
  value === 'app-server' ? 'the application server' : 'the database runtime';

export const FAILED_FINDINGS: Renderers<FailedFindingFields> = {
  'control-missing': f => `${control(f.control)} did not appear`,
  'control-present': f => `${control(f.control)} was shown when it must not be`,
  'control-available': f => `${control(f.control)} stayed available to ${f.actor}`,
  'control-not-ready': f => `${control(f.control)} never became usable for ${names(f.actors)}`,
  'control-blocked': f => `${f.control ? control(f.control) : 'a control'} is covered by another element`,
  'control-empty': f => `${control(f.control)} is empty`,
  'control-unreadable': f => `${control(f.control)} is missing or unreadable for ${names(f.actors)}`,
  'value-mismatch': f => `${control(f.control)} does not show the required value`,
  'text-unexpected': f => `${control(f.control)} shows text that must not appear`,
  'value-unstable': f => `${control(f.control)} changed while nothing happened`,
  'clients-disagree': f => `${names(f.actors)} see different values in ${control(f.control)}`,
  'number-missing': f => `${control(f.control)} shows no number`,
  'number-mismatch': f => `${control(f.control)} reads ${f.observed ?? 'no number'}, `
    + `expected ${expectation(f.expected)}`,
  'count-mismatch': f => `${f.observed} ${f.control} entries shown, expected ${f.expected}`,
  'order-mismatch': f => f.actors?.length
    ? `${names(f.actors)} see ${control(f.control)} entries in different orders`
    : `${control(f.control)} entries are not in the required order`,
  'entries-missing': f => `of ${f.expected} entries, ${f.missing} missing and ${f.duplicated} duplicated`,
  'actors-with-control': f => `${f.observed} actor(s) hold ${control(f.control)}, expected ${f.expected}`,
  'too-many-per-actor': f => `an actor holds more than ${f.maxEach} of ${control(f.control)}`,
  'clicks-failed': f => `${f.failed} of ${f.total} simultaneous clicks on ${control(f.control)} did not go through`,
  'choice-missing': f => `${f.control ? control(f.control) : 'a control'} did not offer the required choice`,
  'page-timeout': f => `${f.control ? control(f.control) : 'the page'} did not respond in time`,
  'page-crashed': () => 'the page crashed',
  'page-error': f => `${f.control ? control(f.control) : 'the page'} did not behave as required`,
  'app-control-failed': f => `${target(f.target)} could not ${f.mode}`,
  'script-failed': f => `${f.script} failed`,
  'script-invalid': f => `${f.script} is not a script inside the application directory`,
  'action-failed': f => `the ${f.action} step did not complete`,
  'call-refused': f => `the ${f.action} action was refused for ${f.actor} (${http(f.status)})${operation(f.operation)}`,
  'call-accepted': f => f.required === 'validation-refused'
    ? `the ${f.action} action accepted invalid input from ${f.actor}`
    : `the ${f.action} action was accepted for ${f.actor}, who must be refused`,
  'call-error': f => `the ${f.action} action failed for ${f.actor} with ${http(f.status)} instead of `
    + `${f.required === 'validation-refused' ? 'rejecting the input' : 'refusing the caller'}${operation(f.operation)}`,
  'concurrent-calls-mismatch': f => `${f.accepted} of ${f.fired} simultaneous ${f.action} calls were accepted, expected ${f.expected}`,
  'interface-missing': f => `${control(f.control)} exposes no ${f.attribute} for the ${f.action} action`,
  'interface-invalid': f => f.missing?.length
    ? `${f.attribute} for the ${f.action} action is missing ${names(f.missing)}`
    : f.unexpected?.length
      ? `${f.attribute} for the ${f.action} action contains unexpected ${names(f.unexpected)}`
      : `${f.attribute} for the ${f.action} action is not valid`,
  'replay-accepted': f => `a request replayed as ${f.actor}, who must be refused, was accepted (${http(f.status)})`,
  'replay-error': f => `the replayed ${f.action ? `${f.action} action` : 'request'} failed with ${http(f.status)} instead of a refusal`,
  'forgery-accepted': f => `a request with a tampered ${f.field} was accepted (${http(f.status)})`,
  'forgery-error': f => `the tampered request failed with ${http(f.status)} instead of a refusal`,
  'message-delivered': f => `a private message was delivered to ${f.actor}, who is not a participant`,
};

export const INCONCLUSIVE_FINDINGS: Renderers<InconclusiveFindingFields> = {
  'assertion-without-action': f => `no ${f.action} ran before this assertion`,
  'unknown-action': f => `the track names no ${f.action} action`,
  'action-without-parameters': f => `the ${f.action} action declares no named parameters`,
  'no-session': f => `no session found for ${f.actor}${f.action ? `, so the ${f.action} action could not be issued` : ''}`,
  'unresolved-action': f => `could not resolve where to send ${f.action ? `the ${f.action} action` : 'the action'} for this backend`,
  'replay-unavailable': f => `could not issue the replay as ${f.actor}`,
  'forgery-unverifiable': f => `could not verify the forgery refusal for ${f.actor}`,
  'not-observed': f => `the message could not be observed reaching ${f.actor}`,
  'nothing-contended': () => 'the requests never contended',
  'no-backend-control': f => `no control over ${target(f.target)} was supplied`,
  'control-refused': f => `control over ${target(f.target)} was refused on this host`,
  'database-write-failed': () => 'the direct database write did not complete',
  'unsupported-backend': f => `${f.backend} does not support this step`,
  'app-directory-unknown': () => 'the application directory is unknown',
  'invalid-input': () => 'the step input is invalid',
};

export const FAILED_FINDING_KINDS = Object.freeze(Object.keys(FAILED_FINDINGS).sort()) as readonly FailedFindingKind[];
export const INCONCLUSIVE_FINDING_KINDS = Object.freeze(
  Object.keys(INCONCLUSIVE_FINDINGS).sort()) as readonly InconclusiveFindingKind[];
export const FINDING_KINDS = Object.freeze([...FAILED_FINDING_KINDS, ...INCONCLUSIVE_FINDING_KINDS].sort()) as readonly FindingKind[];

export function finding<K extends FindingKind>(kind: K, fields: FindingFields[K]): Finding {
  return { kind, fields } as Finding;
}

export function findingStatus(value: Finding): FindingStatus {
  return value.kind in FAILED_FINDINGS ? 'failed' : 'inconclusive';
}

// The one sentence every reader shows for a finding.
export function renderFinding(value: Finding): string {
  const renderers = { ...FAILED_FINDINGS, ...INCONCLUSIVE_FINDINGS } as Renderers<FindingFields>;
  return (renderers[value.kind] as (fields: unknown) => string)(value.fields);
}

export const findingSchema = z.strictObject({
  kind: z.enum(FINDING_KINDS as [FindingKind, ...FindingKind[]]),
  fields: z.record(z.string(), z.unknown()),
});

export function isFinding(value: unknown): value is Finding {
  return findingSchema.safeParse(value).success;
}
