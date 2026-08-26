import type { ElementsObj, Infer as InferBuilder } from 'spacetimedb/server';
import type { ToolDefinition, ResponseFormat } from './openrouter';

type TypeBuilderLike = ElementsObj[string];
type IsUnit<T> = [keyof T] extends [never] ? true : false;

type RunFn<TB extends TypeBuilderLike> =
  IsUnit<InferBuilder<TB>> extends true
    ? (ctx: unknown) => string
    : (ctx: unknown, args: InferBuilder<TB>) => string;

const DESC_KEY = Symbol.for('agents-ts/description');
const RUN_KEY = Symbol.for('agents-ts/run');
const MAX_TOOLS = 64;
const MAX_TOOL_DESCRIPTION_LENGTH = 1024;
const MAX_TOOL_INPUT_JSON_LENGTH = 64 * 1024;
const MAX_TOOL_RESULT_LENGTH = 64 * 1024;
const MAX_TOOL_ARRAY_LENGTH = 1000;

export type AgentTool<TB extends TypeBuilderLike> = TB & {
  [DESC_KEY]: string;
  [RUN_KEY]: RunFn<TB>;
};

export function agentTool<TB extends TypeBuilderLike>(
  description: string,
  args: TB,
  run: RunFn<TB>
): AgentTool<TB> {
  const normalizedDescription = description.trim();
  if (
    normalizedDescription.length === 0 ||
    normalizedDescription.length > MAX_TOOL_DESCRIPTION_LENGTH
  ) {
    throw new Error('agentTool description must contain 1 to 1024 characters');
  }
  Object.defineProperty(args, DESC_KEY, {
    value: normalizedDescription,
    enumerable: false,
    configurable: false,
    writable: false,
  });
  Object.defineProperty(args, RUN_KEY, {
    value: run,
    enumerable: false,
    configurable: false,
    writable: false,
  });
  return args as AgentTool<TB>;
}

export type InvokeResult = { result: string; isError: boolean };

export function makeAgentDispatch<
  Tx,
  T extends Record<string, AgentTool<TypeBuilderLike>>,
>(tools: T) {
  if (Object.keys(tools).length > MAX_TOOLS) {
    throw new Error(`an agent may define at most ${MAX_TOOLS} tools`);
  }
  const llmToolDefs: ToolDefinition[] = [];
  for (const [name, tool] of Object.entries(tools)) {
    if (!isValidToolName(name)) {
      throw new Error(
        `agentTool name '${name}' must match /^[a-zA-Z0-9_-]{1,64}$/`
      );
    }
    llmToolDefs.push({
      type: 'function',
      function: {
        name,
        description: tool[DESC_KEY],
        parameters: typeBuilderToJsonSchema(tool),
      },
    });
  }

  function invoke(ctx: Tx, name: string, inputJson: string): InvokeResult {
    if (!Object.hasOwn(tools, name)) {
      return { result: `unknown tool: ${name}`, isError: true };
    }
    const tool = (tools as Record<string, AgentTool<TypeBuilderLike>>)[name];
    if (!tool) return { result: `unknown tool: ${name}`, isError: true };

    if (inputJson.length > MAX_TOOL_INPUT_JSON_LENGTH) {
      return { result: 'tool input exceeds 65536 characters', isError: true };
    }
    let parsed: unknown;
    try {
      const decoded: unknown = inputJson === '' ? {} : JSON.parse(inputJson);
      parsed = validateToolValue(tool.algebraicType, decoded, '$');
    } catch (err) {
      return {
        result: `invalid JSON in tool input: ${err instanceof Error ? err.message : String(err)}`,
        isError: true,
      };
    }

    try {
      const run = tool[RUN_KEY] as (ctx: Tx, args: unknown) => string;
      const result = run(ctx, parsed);
      if (typeof result !== 'string') {
        return { result: 'tool returned a non-string result', isError: true };
      }
      if (result.length > MAX_TOOL_RESULT_LENGTH) {
        return {
          result: 'tool result exceeds 65536 characters',
          isError: true,
        };
      }
      return { result, isError: false };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        result: message.slice(0, MAX_TOOL_RESULT_LENGTH),
        isError: true,
      };
    }
  }

  return { llmToolDefs, invoke };
}

function isValidToolName(name: string): boolean {
  return /^[a-zA-Z0-9_-]{1,64}$/.test(name);
}

type AlgebraicTypeLike = { tag: string; value?: unknown };
type AlgebraicElement = { name: string; algebraicType: AlgebraicTypeLike };
type AlgebraicVariant = { name: string; algebraicType: AlgebraicTypeLike };

function objectValue(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function productElements(at: AlgebraicTypeLike): AlgebraicElement[] {
  const value = objectValue(at.value);
  return Array.isArray(value?.elements)
    ? (value.elements as AlgebraicElement[])
    : [];
}

function sumVariants(at: AlgebraicTypeLike): AlgebraicVariant[] {
  const value = objectValue(at.value);
  return Array.isArray(value?.variants)
    ? (value.variants as AlgebraicVariant[])
    : [];
}

function isUnitType(at: AlgebraicTypeLike): boolean {
  return at.tag === 'Product' && productElements(at).length === 0;
}

function optionPayload(at: AlgebraicTypeLike): AlgebraicTypeLike | undefined {
  if (at.tag !== 'Sum') return undefined;
  const variants = sumVariants(at);
  if (variants.length !== 2) return undefined;
  const unitIndex = variants.findIndex(variant =>
    isUnitType(variant.algebraicType)
  );
  return unitIndex < 0
    ? undefined
    : variants[unitIndex === 0 ? 1 : 0]?.algebraicType;
}

function invalidToolValue(path: string, expected: string): never {
  throw new Error(`invalid tool input: ${path} must be ${expected}`);
}

function validateInteger(
  at: AlgebraicTypeLike,
  value: unknown,
  path: string
): number | bigint {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    return invalidToolValue(path, 'a safe integer');
  }
  const bounds: Record<string, readonly [number, number]> = {
    I8: [-128, 127],
    U8: [0, 255],
    I16: [-32768, 32767],
    U16: [0, 65535],
    I32: [-2147483648, 2147483647],
    U32: [0, 4294967295],
  };
  const bound = bounds[at.tag];
  if (bound && (value < bound[0] || value > bound[1])) {
    return invalidToolValue(path, `within the ${at.tag} range`);
  }
  if (at.tag === 'I64' || at.tag === 'U64') {
    if (at.tag === 'U64' && value < 0)
      return invalidToolValue(path, 'a non-negative safe integer');
    return BigInt(value);
  }
  return value;
}

function validateToolValue(
  at: AlgebraicTypeLike,
  value: unknown,
  path: string
): unknown {
  switch (at.tag) {
    case 'Bool':
      return typeof value === 'boolean'
        ? value
        : invalidToolValue(path, 'a boolean');
    case 'String':
      return typeof value === 'string'
        ? value
        : invalidToolValue(path, 'a string');
    case 'F32':
    case 'F64':
      return typeof value === 'number' && Number.isFinite(value)
        ? value
        : invalidToolValue(path, 'a finite number');
    case 'I8':
    case 'I16':
    case 'I32':
    case 'I64':
    case 'U8':
    case 'U16':
    case 'U32':
    case 'U64':
      return validateInteger(at, value, path);
    case 'Product': {
      const input = objectValue(value);
      if (!input) return invalidToolValue(path, 'an object');
      const elements = productElements(at);
      const names = new Set(elements.map(element => element.name));
      for (const key of Object.keys(input)) {
        if (!names.has(key))
          throw new Error(`invalid tool input: ${path}.${key} is not allowed`);
      }
      const output: Record<string, unknown> = {};
      for (const element of elements) {
        if (!Object.hasOwn(input, element.name)) {
          if (optionPayload(element.algebraicType) !== undefined) continue;
          throw new Error(
            `invalid tool input: ${path}.${element.name} is required`
          );
        }
        const payload = optionPayload(element.algebraicType);
        output[element.name] = validateToolValue(
          payload ?? element.algebraicType,
          input[element.name],
          `${path}.${element.name}`
        );
      }
      return output;
    }
    case 'Array': {
      if (!Array.isArray(value)) return invalidToolValue(path, 'an array');
      if (value.length > MAX_TOOL_ARRAY_LENGTH) {
        throw new Error(
          `invalid tool input: ${path} exceeds ${MAX_TOOL_ARRAY_LENGTH} items`
        );
      }
      const inner = at.value as AlgebraicTypeLike;
      return value.map((item, index) =>
        validateToolValue(inner, item, `${path}[${index}]`)
      );
    }
    case 'Sum': {
      const payload = optionPayload(at);
      if (payload !== undefined) return validateToolValue(payload, value, path);
      const input = objectValue(value);
      if (!input || typeof input.tag !== 'string') {
        return invalidToolValue(path, 'a tagged object');
      }
      const variant = sumVariants(at).find(
        candidate => candidate.name === input.tag
      );
      if (!variant)
        throw new Error(`invalid tool input: ${path}.tag is unknown`);
      const allowed = isUnitType(variant.algebraicType)
        ? new Set(['tag'])
        : new Set(['tag', 'value']);
      for (const key of Object.keys(input)) {
        if (!allowed.has(key))
          throw new Error(`invalid tool input: ${path}.${key} is not allowed`);
      }
      if (isUnitType(variant.algebraicType)) return { tag: input.tag };
      if (!Object.hasOwn(input, 'value'))
        throw new Error(`invalid tool input: ${path}.value is required`);
      return {
        tag: input.tag,
        value: validateToolValue(
          variant.algebraicType,
          input.value,
          `${path}.value`
        ),
      };
    }
    case 'Ref':
      throw new Error(
        'invalid tool input: referenced argument types are unsupported'
      );
    default:
      throw new Error(`invalid tool input: unsupported type ${at.tag}`);
  }
}

export type ToolMap = Record<string, AgentTool<TypeBuilderLike>>;

// Built-ins: 'openrouter' | 'openai' | 'anthropic'.
export type ProviderName = string;

export interface AgentDefinition<TM extends ToolMap = ToolMap> {
  defaultProvider: ProviderName;
  defaultModel: string;
  defaultSystemPrompt: string | undefined;
  defaultMaxTurns: number;
  defaultMaxHistoryMessages: number;
  defaultMaxTokens: number | undefined;
  defaultRetries: number;
  defaultResponseFormat: ResponseFormat | undefined;
  summarizerAgentName: string | undefined;
  embeddingsProvider: string | undefined;
  embeddingsModel: string | undefined;
  ragTopK: number;
  tools: TM;
}

export function defineAgent<TM extends ToolMap>(config: {
  defaultProvider?: ProviderName;
  defaultModel: string;
  defaultSystemPrompt?: string;
  defaultMaxTurns?: number;
  defaultMaxHistoryMessages?: number;
  defaultMaxTokens?: number;
  defaultRetries?: number;
  defaultResponseFormat?: ResponseFormat;
  summarizerAgentName?: string;
  embeddingsProvider?: string;
  embeddingsModel?: string;
  ragTopK?: number;
  tools: TM;
}): AgentDefinition<TM> {
  const provider = validateConfigString(
    config.defaultProvider ?? 'openrouter',
    'defaultProvider',
    64
  );
  const model = validateConfigString(config.defaultModel, 'defaultModel', 256);
  if (
    config.defaultSystemPrompt !== undefined &&
    config.defaultSystemPrompt.length > 32 * 1024
  ) {
    throw new Error('defaultSystemPrompt exceeds 32768 characters');
  }
  const maxTurns = validateConfigInteger(
    config.defaultMaxTurns ?? 10,
    'defaultMaxTurns',
    1,
    100
  );
  const maxHistory = validateConfigInteger(
    config.defaultMaxHistoryMessages ?? 50,
    'defaultMaxHistoryMessages',
    0,
    1000
  );
  const maxTokens =
    config.defaultMaxTokens === undefined
      ? undefined
      : validateConfigInteger(
          config.defaultMaxTokens,
          'defaultMaxTokens',
          1,
          1_000_000
        );
  const retries = validateConfigInteger(
    config.defaultRetries ?? 2,
    'defaultRetries',
    0,
    10
  );
  const ragTopK = validateConfigInteger(config.ragTopK ?? 0, 'ragTopK', 0, 100);
  return {
    defaultProvider: provider,
    defaultModel: model,
    defaultSystemPrompt: config.defaultSystemPrompt,
    defaultMaxTurns: maxTurns,
    defaultMaxHistoryMessages: maxHistory,
    defaultMaxTokens: maxTokens,
    defaultRetries: retries,
    defaultResponseFormat: config.defaultResponseFormat,
    summarizerAgentName:
      config.summarizerAgentName === undefined
        ? undefined
        : validateConfigString(
            config.summarizerAgentName,
            'summarizerAgentName',
            64
          ),
    embeddingsProvider:
      config.embeddingsProvider === undefined
        ? undefined
        : validateConfigString(
            config.embeddingsProvider,
            'embeddingsProvider',
            64
          ),
    embeddingsModel:
      config.embeddingsModel === undefined
        ? undefined
        : validateConfigString(config.embeddingsModel, 'embeddingsModel', 256),
    ragTopK,
    tools: config.tools,
  };
}

function validateConfigString(
  value: string,
  field: string,
  maxLength: number
): string {
  const normalized = value.trim();
  if (normalized.length === 0 || normalized.length > maxLength) {
    throw new Error(`${field} must contain 1 to ${maxLength} characters`);
  }
  return normalized;
}

function validateConfigInteger(
  value: number,
  field: string,
  minimum: number,
  maximum: number
): number {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(
      `${field} must be an integer from ${minimum} to ${maximum}`
    );
  }
  return value;
}

export interface AgentRegistry<Tx> {
  has(agentName: string): boolean;
  names(): string[];
  agentDef(agentName: string): AgentDefinition | undefined;
  llmToolDefsFor(agentName: string): ToolDefinition[];
  invoke(
    agentName: string,
    ctx: Tx,
    toolName: string,
    inputJson: string
  ): InvokeResult;
}

export function makeAgentRegistry<
  Tx,
  Agents extends Record<string, AgentDefinition<ToolMap>>,
>(agents: Agents): AgentRegistry<Tx> {
  const dispatches = new Map<
    string,
    ReturnType<typeof makeAgentDispatch<Tx, ToolMap>>
  >();
  for (const [name, def] of Object.entries(agents)) {
    if (!isValidAgentName(name)) {
      throw new Error(
        `agent name '${name}' must match /^[a-zA-Z0-9_-]{1,64}$/`
      );
    }
    dispatches.set(name, makeAgentDispatch<Tx, typeof def.tools>(def.tools));
  }

  return {
    has(agentName: string): boolean {
      return Object.hasOwn(agents, agentName);
    },
    names(): string[] {
      return Object.keys(agents);
    },
    agentDef(agentName: string): AgentDefinition | undefined {
      return Object.hasOwn(agents, agentName) ? agents[agentName] : undefined;
    },
    llmToolDefsFor(agentName: string): ToolDefinition[] {
      const d = dispatches.get(agentName);
      if (!d) return [];
      return d.llmToolDefs;
    },
    invoke(
      agentName: string,
      ctx: Tx,
      toolName: string,
      inputJson: string
    ): InvokeResult {
      const d = dispatches.get(agentName);
      if (!d) return { result: `unknown agent: ${agentName}`, isError: true };
      return d.invoke(ctx, toolName, inputJson);
    },
  };
}

function isValidAgentName(name: string): boolean {
  return /^[a-zA-Z0-9_-]{1,64}$/.test(name);
}

export function typeBuilderToJsonSchema(tb: TypeBuilderLike): {
  type: 'object';
  properties: Record<string, unknown>;
  required?: string[];
} {
  const at = tb.algebraicType;
  if (!at || at.tag !== 'Product') {
    throw new Error(
      `agentTool args must be a t.object(...) or t.unit(); got ${at?.tag ?? 'unknown'}`
    );
  }
  return algebraicProductToObjectSchema(at);
}

function algebraicProductToObjectSchema(at: AlgebraicTypeLike): {
  type: 'object';
  properties: Record<string, unknown>;
  required?: string[];
} {
  const properties: Record<string, unknown> = {};
  const required: string[] = [];
  const elements = productElements(at);
  for (const el of elements) {
    const inner = algebraicTypeToJsonSchema(el.algebraicType);
    properties[el.name] = inner.schema;
    if (inner.required) required.push(el.name);
  }
  const out: {
    type: 'object';
    properties: Record<string, unknown>;
    required?: string[];
  } = {
    type: 'object',
    properties,
  };
  if (required.length > 0) out.required = required;
  return out;
}

function algebraicTypeToJsonSchema(at: AlgebraicTypeLike): {
  schema: unknown;
  required: boolean;
} {
  switch (at.tag) {
    case 'Bool':
      return { schema: { type: 'boolean' }, required: true };
    case 'String':
      return { schema: { type: 'string' }, required: true };
    case 'F32':
    case 'F64':
      return { schema: { type: 'number' }, required: true };
    case 'I8':
    case 'I16':
    case 'I32':
    case 'I64':
    case 'U8':
    case 'U16':
    case 'U32':
    case 'U64':
      return { schema: { type: 'integer' }, required: true };
    case 'I128':
    case 'U128':
    case 'I256':
    case 'U256':
      throw new Error(
        `tool arg type ${at.tag} is not representable in JSON Schema; use a smaller integer type or t.string()`
      );
    case 'Product':
      return { schema: algebraicProductToObjectSchema(at), required: true };
    case 'Array': {
      const inner = algebraicTypeToJsonSchema(at.value as AlgebraicTypeLike);
      return { schema: { type: 'array', items: inner.schema }, required: true };
    }
    case 'Sum': {
      const variants = sumVariants(at);
      // Unwrap option<T> = Sum { some: T, none: () }.
      if (variants.length === 2) {
        const unitVariantIdx = variants.findIndex(v =>
          isUnitType(v.algebraicType)
        );
        const payloadIdx =
          unitVariantIdx === 0 ? 1 : unitVariantIdx === 1 ? 0 : -1;
        if (unitVariantIdx >= 0 && payloadIdx >= 0) {
          const inner = algebraicTypeToJsonSchema(
            variants[payloadIdx].algebraicType
          );
          return { schema: inner.schema, required: false };
        }
      }
      return {
        schema: {
          oneOf: variants.map(v => {
            const inner = algebraicTypeToJsonSchema(v.algebraicType);
            return {
              type: 'object',
              properties: {
                tag: { type: 'string', enum: [v.name] },
                value: inner.schema,
              },
              required: ['tag'],
            };
          }),
        },
        required: true,
      };
    }
    case 'Ref':
      throw new Error(
        'typespace Ref types are not supported in tool args; declare the type inline with t.object(...)'
      );
    default:
      throw new Error(`unsupported algebraic type tag: ${at.tag}`);
  }
}
