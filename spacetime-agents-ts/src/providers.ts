import type {
  ChatRequest,
  ChatMessage,
  ContentBlock,
  ToolCall,
  Provider,
  ParsedResponse,
} from './openrouter.ts';

type JsonObject = Record<string, unknown>;

function asObject(value: unknown): JsonObject | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as JsonObject)
    : undefined;
}

function tokenCount(value: unknown): number {
  const count = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(count) && count >= 0
    ? Math.min(Math.trunc(count), Number.MAX_SAFE_INTEGER)
    : 0;
}

function toOpenAiContent(content: string | ContentBlock[]): unknown {
  if (typeof content === 'string') return content;
  return content.map(b =>
    b.type === 'text'
      ? { type: 'text', text: b.text }
      : {
          type: 'image_url',
          image_url: { url: `data:${b.mimeType};base64,${b.data}` },
        }
  );
}

function toOpenAiMessage(m: ChatMessage): unknown {
  if (m.role === 'tool') return m;
  const out: JsonObject = { role: m.role, content: toOpenAiContent(m.content) };
  if (m.tool_calls) out.tool_calls = m.tool_calls;
  return out;
}

function buildOpenAiBody(req: ChatRequest): unknown {
  const messages = req.messages.map(toOpenAiMessage);
  const body: Record<string, unknown> = {
    model: req.model,
    messages: req.system
      ? [{ role: 'system', content: req.system }, ...messages]
      : messages,
  };
  if (req.tools && req.tools.length > 0) {
    body.tools = req.tools;
    body.tool_choice = 'auto';
  }
  if (req.maxTokens !== undefined) body.max_tokens = req.maxTokens;
  if (req.responseFormat !== undefined)
    body.response_format = req.responseFormat;
  return body;
}

function parseOpenAiResponse(
  text: string,
  requestedModel: string
): ParsedResponse {
  const parsed: unknown = JSON.parse(text);
  const root = asObject(parsed);
  const choices = root?.choices;
  const choice = Array.isArray(choices) ? asObject(choices[0]) : undefined;
  if (!choice) throw new Error('no choices in response');
  const message = asObject(choice.message) ?? {};
  const toolCalls: ToolCall[] = Array.isArray(message.tool_calls)
    ? message.tool_calls.map(value => {
        const toolCall = asObject(value) ?? {};
        const fn = asObject(toolCall.function) ?? {};
        return {
          id: String(toolCall.id ?? ''),
          type: 'function',
          function: {
            name: String(fn.name ?? ''),
            arguments: String(fn.arguments ?? '{}'),
          },
        };
      })
    : [];
  const usage = asObject(root?.usage) ?? {};
  return {
    text: typeof message.content === 'string' ? message.content : null,
    toolCalls,
    finishReason: String(choice.finish_reason ?? 'stop'),
    usage: {
      promptTokens: tokenCount(usage.prompt_tokens),
      completionTokens: tokenCount(usage.completion_tokens),
      totalTokens: tokenCount(usage.total_tokens),
    },
    model: String(root?.model ?? requestedModel),
    raw: parsed,
  };
}

export const openRouterProvider: Provider = {
  name: 'openrouter',
  buildRequest(req) {
    return {
      url: 'https://openrouter.ai/api/v1/chat/completions',
      headers: {
        Authorization: `Bearer ${req.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(buildOpenAiBody(req)),
    };
  },
  parseResponse: parseOpenAiResponse,
};

export const openAiProvider: Provider = {
  name: 'openai',
  buildRequest(req) {
    return {
      url: 'https://api.openai.com/v1/chat/completions',
      headers: {
        Authorization: `Bearer ${req.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(buildOpenAiBody(req)),
    };
  },
  parseResponse: parseOpenAiResponse,
};

type AnthropicContentBlock =
  | { type: 'text'; text: string }
  | {
      type: 'image';
      source: { type: 'base64'; media_type: string; data: string };
    };

function toAnthropicBlocks(
  content: string | ContentBlock[]
): AnthropicContentBlock[] {
  if (typeof content === 'string') return [{ type: 'text', text: content }];
  return content.map(b =>
    b.type === 'text'
      ? { type: 'text', text: b.text }
      : {
          type: 'image',
          source: { type: 'base64', media_type: b.mimeType, data: b.data },
        }
  );
}

export const anthropicProvider: Provider = {
  name: 'anthropic',
  buildRequest(req) {
    const messages: JsonObject[] = [];
    for (const m of req.messages) {
      if (m.role === 'tool') {
        messages.push({
          role: 'user',
          content: [
            {
              type: 'tool_result',
              tool_use_id: m.tool_call_id,
              content: m.content,
            },
          ],
        });
      } else if (
        m.role === 'assistant' &&
        m.tool_calls &&
        m.tool_calls.length > 0
      ) {
        const content: unknown[] = [];
        const textBlocks = toAnthropicBlocks(m.content);
        for (const b of textBlocks) {
          if (b.type === 'text' && !b.text) continue;
          content.push(b);
        }
        for (const tc of m.tool_calls) {
          let input: unknown = {};
          try {
            input = JSON.parse(tc.function.arguments);
          } catch {
            /* Preserve the empty fallback. */
          }
          content.push({
            type: 'tool_use',
            id: tc.id,
            name: tc.function.name,
            input,
          });
        }
        messages.push({ role: 'assistant', content });
      } else if (m.role === 'system') {
        continue;
      } else {
        messages.push({
          role: m.role,
          content:
            typeof m.content === 'string'
              ? m.content
              : toAnthropicBlocks(m.content),
        });
      }
    }

    const body: Record<string, unknown> = {
      model: req.model,
      messages,
      max_tokens: req.maxTokens ?? 4096,
    };
    if (req.system) body.system = req.system;
    if (req.tools && req.tools.length > 0) {
      body.tools = req.tools.map(t => ({
        name: t.function.name,
        description: t.function.description,
        input_schema: t.function.parameters,
      }));
      body.tool_choice = { type: 'auto' };
    }
    return {
      url: 'https://api.anthropic.com/v1/messages',
      headers: {
        'x-api-key': req.apiKey,
        'anthropic-version': '2023-06-01',
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    };
  },
  parseResponse(text, requestedModel) {
    const parsed: unknown = JSON.parse(text);
    const root = asObject(parsed);
    if (!Array.isArray(root?.content))
      throw new Error('no content in anthropic response');

    let textContent: string | null = null;
    const toolCalls: ToolCall[] = [];
    for (const value of root.content) {
      const block = asObject(value);
      if (!block) continue;
      if (block.type === 'text') {
        textContent = (textContent ?? '') + String(block.text ?? '');
      } else if (block.type === 'tool_use') {
        toolCalls.push({
          id: String(block.id ?? ''),
          type: 'function',
          function: {
            name: String(block.name ?? ''),
            arguments: JSON.stringify(block.input ?? {}),
          },
        });
      }
    }

    const stopReason = String(root.stop_reason ?? 'end_turn');
    const finishReason =
      stopReason === 'end_turn'
        ? 'stop'
        : stopReason === 'tool_use'
          ? 'tool_calls'
          : stopReason === 'max_tokens'
            ? 'length'
            : stopReason === 'stop_sequence'
              ? 'stop'
              : stopReason;

    const usage = asObject(root.usage) ?? {};
    const inputTokens = tokenCount(usage.input_tokens);
    const outputTokens = tokenCount(usage.output_tokens);
    return {
      text: textContent,
      toolCalls,
      finishReason,
      usage: {
        promptTokens: inputTokens,
        completionTokens: outputTokens,
        totalTokens: inputTokens + outputTokens,
      },
      model: String(root.model ?? requestedModel),
      raw: parsed,
    };
  },
};

export const BUILT_IN_PROVIDERS: Record<string, Provider> = {
  openrouter: openRouterProvider,
  openai: openAiProvider,
  anthropic: anthropicProvider,
};
