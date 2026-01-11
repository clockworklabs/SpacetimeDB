/**
 * Type-level tests for column metadata validation.
 *
 * These tests verify that invalid combinations of column attributes
 * (like default + primaryKey) produce compile-time errors.
 *
 * To run these tests, simply run `pnpm tsc --noEmit` on this file.
 * The tests use @ts-expect-error to assert that certain combinations
 * should produce type errors.
 */

import { describe, it, expect } from 'vitest';
import { t } from '../src/lib/type_builders';
import { table } from '../src/lib/table';

// ============================================================
// VALID COMBINATIONS - These should compile without errors
// ============================================================

// Valid: default alone
const validDefault = table(
  { name: 'valid_default' },
  {
    id: t.u64().primaryKey(),
    score: t.u32().default(0),
  }
);

// Valid: primaryKey alone
const validPrimaryKey = table(
  { name: 'valid_primary_key' },
  {
    id: t.u64().primaryKey(),
    name: t.string(),
  }
);

// Valid: unique alone
const validUnique = table(
  { name: 'valid_unique' },
  {
    id: t.u64().primaryKey(),
    email: t.string().unique(),
  }
);

// Valid: autoInc alone
const validAutoInc = table(
  { name: 'valid_auto_inc' },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

// Valid: index with default
const validIndexWithDefault = table(
  { name: 'valid_index_default' },
  {
    id: t.u64().primaryKey(),
    score: t.u32().index().default(0),
  }
);

// ============================================================
// INVALID COMBINATIONS - These should produce compile errors
// ============================================================

// Invalid: default + primaryKey
// @ts-expect-error - default() cannot be combined with primaryKey()
const invalidDefaultPrimaryKey = table(
  { name: 'invalid_default_pk' },
  {
    id: t.u64().default(0n).primaryKey(),
    name: t.string(),
  }
);

// Invalid: primaryKey + default
// @ts-expect-error - primaryKey() cannot be combined with default()
const invalidPrimaryKeyDefault = table(
  { name: 'invalid_pk_default' },
  {
    id: t.u64().primaryKey().default(0n),
    name: t.string(),
  }
);

// Invalid: default + unique
// @ts-expect-error - default() cannot be combined with unique()
const invalidDefaultUnique = table(
  { name: 'invalid_default_unique' },
  {
    id: t.u64().primaryKey(),
    email: t.string().default('').unique(),
  }
);

// Invalid: unique + default
// @ts-expect-error - unique() cannot be combined with default()
const invalidUniqueDefault = table(
  { name: 'invalid_unique_default' },
  {
    id: t.u64().primaryKey(),
    email: t.string().unique().default(''),
  }
);

// Invalid: default + autoInc
// @ts-expect-error - default() cannot be combined with autoInc()
const invalidDefaultAutoInc = table(
  { name: 'invalid_default_autoinc' },
  {
    id: t.u64().default(0n).autoInc(),
    name: t.string(),
  }
);

// Invalid: autoInc + default
// @ts-expect-error - autoInc() cannot be combined with default()
const invalidAutoIncDefault = table(
  { name: 'invalid_autoinc_default' },
  {
    id: t.u64().autoInc().default(0n),
    name: t.string(),
  }
);

// Suppress unused variable warnings
void validDefault;
void validPrimaryKey;
void validUnique;
void validAutoInc;
void validIndexWithDefault;
void invalidDefaultPrimaryKey;
void invalidPrimaryKeyDefault;
void invalidDefaultUnique;
void invalidUniqueDefault;
void invalidDefaultAutoInc;
void invalidAutoIncDefault;

describe('Column metadata validation', () => {
  it('type-level tests compile correctly (see @ts-expect-error comments above)', () => {
    // This test exists to satisfy vitest - the actual validation happens
    // at compile time via @ts-expect-error annotations above.
    // If this file compiles, the type-level tests have passed.
    expect(true).toBe(true);
  });
});
