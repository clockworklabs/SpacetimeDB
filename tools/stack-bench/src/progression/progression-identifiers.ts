import { isExactSemanticVersion, parseVersionedReference } from '../semantic-version.js';

const IDENTIFIER = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;

export const isProgressionIdentifier = (value: string): boolean => IDENTIFIER.test(value);
export const isProgressionVersion = isExactSemanticVersion;

export function parseVersionedProgressionId(value: string): { id: string; version: string } | null {
  return parseVersionedReference(value, isProgressionIdentifier);
}
