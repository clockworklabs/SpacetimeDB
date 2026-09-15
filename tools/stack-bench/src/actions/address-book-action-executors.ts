import { ActionApplicationFailure, actionImplementation } from './action-contract.js';
import { actorFor, inconclusive } from './actor-action-runtime.js';
import type { ActorCapabilities, ActorActionArguments, HeaderRecord } from './actor-action-runtime.js';
import { browserCredentials, capturedCredentials } from './named-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { addressEntriesSchema, type AddressEntry } from '../stacks/migration-state.js';
import type { AddressBookRead } from '../stacks/address-book-read.js';

interface Input {
  actor: string;
  authentication?: 'actor' | 'none';
  as?: string;
  entryName?: string;
  sameAs?: string;
  entries?: Array<Omit<AddressEntry, 'id'> & { idFrom?: string }>;
}
interface Capabilities extends ActorCapabilities {
  readonly 'address-book-read': {
    read(credentials: HeaderRecord, signal: AbortSignal, recordResponse: (text: string) => void): Promise<AddressBookRead>;
  };
  readonly 'browser-observation': { recorded: { get(key: string): unknown; set(key: string, value: unknown): void } };
}
type Arguments = ActorActionArguments<Input, Capabilities>;

async function read({ input, capabilities, signal }: Arguments) {
  const actor = actorFor(capabilities, input.actor);
  const credentials = input.authentication === 'none' ? {}
    : capturedCredentials(actor) ?? await browserCredentials(actor);
  if (!credentials) inconclusive('no-session', { actor: input.actor, action: 'read address book' });
  return capabilities['address-book-read'].read(credentials, signal, text => actor.record(text));
}

async function recordAddressBook(args: Arguments) {
  const observation = await read(args);
  if (observation.entries === null) throw new ActionApplicationFailure('owner address-book read was refused', { observation });
  const entries = args.input.entryName === undefined ? observation.entries
    : observation.entries.filter(entry => entry.name === args.input.entryName);
  if (args.input.entryName !== undefined && entries.length !== 1) {
    throw new ActionApplicationFailure('address-book ID capture requires exactly one entry with the selected name', { observation });
  }
  args.capabilities['browser-observation'].recorded.set(args.input.as!, entries);
  return observation;
}

async function expectAddressBook(args: Arguments) {
  const observation = await read(args);
  const { input, capabilities } = args;
  const saved = (key: string) => addressEntriesSchema.parse(capabilities['browser-observation'].recorded.get(key));
  if (observation.entries === null) {
    if (input.authentication === 'none' && input.entries?.length === 0) return observation;
    throw new ActionApplicationFailure('owner address-book read was refused', { observation });
  }
  const entries = observation.entries;
  const expected = input.sameAs !== undefined ? saved(input.sameAs) : input.entries!.map(({ idFrom, ...entry }) => {
    if (idFrom === undefined) return { ...entry, id: undefined };
    const prior = saved(idFrom);
    if (prior.length !== 1) throw new Error('idFrom requires a saved single-entry observation');
    return { ...entry, id: prior[0]!.id };
  });
  const remaining = [...entries];
  // Match bound IDs first so an otherwise identical unbound row cannot consume them.
  const matches = [...expected].sort((a, b) => Number(b.id !== undefined) - Number(a.id !== undefined)).every(value => {
    const index = remaining.findIndex(entry => (value.id === undefined || entry.id === value.id)
      && entry.name === value.name && entry.address === value.address && entry.isDefault === value.isDefault);
    if (index < 0) return false;
    remaining.splice(index, 1);
    return true;
  });
  if (new Set(entries.map(entry => entry.id)).size !== entries.length
    || !matches || remaining.length) {
    throw new ActionApplicationFailure('address-book entries differ from the expected saved values',
      { observation: { ...observation, expected } });
  }
  return observation;
}

export const ADDRESS_BOOK_ACTION_IMPLEMENTATIONS = Object.freeze({
  recordAddressBook: actionImplementation(browserApplicationBoundary(recordAddressBook)),
  expectAddressBook: actionImplementation(browserApplicationBoundary(expectAddressBook)),
});
