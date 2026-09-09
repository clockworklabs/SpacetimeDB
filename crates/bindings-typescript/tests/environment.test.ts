import { describe, expect, expectTypeOf, it, vi } from 'vitest';
import { schema } from '../src/server/schema';
import { t } from '../src/lib/type_builders';
import {
  environment,
  environmentDeclarations,
} from '../src/server/environment';
import type { EnvironmentSchema } from '../src/lib/environment';
import { env_get } from 'spacetime:sys@2.3';

vi.mock('spacetime:sys@2.3', async importOriginal => ({
  ...(await importOriginal<object>()),
  env_get: vi.fn(),
}));

const declarations = {
  FOOBAR: t.string(),
  ENABLE_EMAIL: t.enum('EnableEmail', ['true', 'false']),
  LOG_LEVEL: t.enum('LogLevel', ['debug', 'info', 'error']).optional(),
  DEPLOYMENT_KIND: t.enum('DeploymentKind', ['production']),
  get: t.string().optional(),
};

describe('declared database environment', () => {
  it('emits canonical constraint metadata and an explicit empty section', () => {
    expect(environmentDeclarations(declarations)).toEqual([
      { name: 'FOOBAR', constraint: { tag: 'AnyString' }, optional: false },
      {
        name: 'ENABLE_EMAIL',
        constraint: { tag: 'OneOf', value: ['true', 'false'] },
        optional: false,
      },
      {
        name: 'LOG_LEVEL',
        constraint: { tag: 'OneOf', value: ['debug', 'info', 'error'] },
        optional: true,
      },
      {
        name: 'DEPLOYMENT_KIND',
        constraint: { tag: 'Literal', value: 'production' },
        optional: false,
      },
      { name: 'get', constraint: { tag: 'AnyString' }, optional: true },
    ]);
    const defined = schema({}, { env: declarations });
    const moduleDef = defined.buildRawModuleDefV10({});
    expect(moduleDef.sections).toContainEqual({
      tag: 'Capabilities',
      value: ['hosted_auth_v1'],
    });
    const section = moduleDef.sections.find(
      section => section.tag === 'Environment'
    );
    expect(section).toEqual({
      tag: 'Environment',
      value: environmentDeclarations(declarations),
    });
    expect(schema({}).buildRawModuleDefV10({}).sections).toContainEqual({
      tag: 'Environment',
      value: [],
    });
    // Enum values outside env retain their existing tagged-sum interpretation.
    expect(Reflect.get(declarations.ENABLE_EMAIL, 'true')).toEqual({
      tag: 'true',
    });
  });

  it('rejects unsupported constraints and submodule declarations before upload', () => {
    const invalid = (value: unknown) => () =>
      environmentDeclarations(value as EnvironmentSchema);
    expect(invalid({ BAD: t.u32() })).toThrow('string or simple enum');
    expect(invalid({ BAD: t.enum('Payload', { value: t.string() }) })).toThrow(
      'enum payload'
    );
    expect(invalid({ BAD: t.enum('Empty', []) })).toThrow(
      'nonempty literal union'
    );
    expect(invalid({ BAD: t.string().optional().optional() })).toThrow();
    expect(invalid({ 'bad-name': t.string() })).toThrow('name');
    expect(invalid({ BAD: t.enum('Long', ['x'.repeat(8193)]) })).toThrow(
      'too long'
    );
    expect(
      invalid(
        Object.fromEntries(
          Array.from({ length: 257 }, (_, i) => [`K${i}`, t.string()])
        )
      )
    ).toThrow('Too many');
    expect(() =>
      schema({ child: { default: schema({}, { env: declarations }) } })
    ).toThrow('Submodules');
    expect(() =>
      schema({ child: { default: schema({}, { env: {} }) } })
    ).not.toThrow();
  });

  it('preserves generic get, optional absence, host errors and uncached named reads', () => {
    const get = vi.mocked(env_get);
    get.mockReset();
    get
      .mockReturnValueOnce('first')
      .mockReturnValueOnce('')
      .mockReturnValueOnce(null)
      .mockReturnValueOnce('declared get');
    const named = environment as typeof environment & {
      readonly FOOBAR: string;
      readonly LOG_LEVEL: string | undefined;
    };
    expect(named.FOOBAR).toBe('first');
    expect(named.FOOBAR).toBe('');
    expect(named.LOG_LEVEL).toBeUndefined();
    expect(named.get('get')).toBe('declared get');
    get.mockReturnValueOnce(null);
    expect(named.get('LOG_LEVEL')).toBeNull();
    get.mockImplementationOnce(() => {
      throw new Error('undeclared host key');
    });
    expect(() => named.get('UNDECLARED')).toThrow('undeclared host key');
    expect(get.mock.calls.map(([name]) => name)).toEqual([
      'FOOBAR',
      'FOOBAR',
      'LOG_LEVEL',
      'get',
      'LOG_LEVEL',
      'UNDECLARED',
    ]);
  });
});

// These declarations are compiled by the focused typecheck. Callback bodies
// need not execute to assert their schema-specific context types.
const typed = schema({}, { env: declarations });
typed.reducer(ctx => {
  expectTypeOf(ctx.env.FOOBAR).toEqualTypeOf<string>();
  expectTypeOf(ctx.env.ENABLE_EMAIL).toEqualTypeOf<'true' | 'false'>();
  expectTypeOf(ctx.env.LOG_LEVEL).toEqualTypeOf<
    'debug' | 'info' | 'error' | undefined
  >();
  expectTypeOf(ctx.env.DEPLOYMENT_KIND).toEqualTypeOf<'production'>();
  expectTypeOf(ctx.env.get('FOOBAR')).toEqualTypeOf<string>();
  expectTypeOf(ctx.env.get('get')).toEqualTypeOf<string | null>();
  // @ts-expect-error Undeclared literal names have no checked accessor.
  ctx.env.get('UNDECLARED');
  // @ts-expect-error No named access to undeclared keys.
  void ctx.env.UNKNOWN;
  // @ts-expect-error Environment access is read-only.
  ctx.env.FOOBAR = 'changed';
});
function rejectPayloadType() {
  // @ts-expect-error Payload enums cannot declare environment strings.
  schema({}, { env: { BAD: t.enum('Payload', { value: t.string() }) } });
}
void rejectPayloadType;
