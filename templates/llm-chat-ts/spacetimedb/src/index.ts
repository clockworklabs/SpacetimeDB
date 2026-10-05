import {
  schema,
  table,
  t,
  SenderError,
  type ReducerCtx,
} from 'spacetimedb/server';
import {
  callChat,
  formatChatError,
  providers,
  type ChatMessage,
} from './llm';

const MAX_USER_MESSAGE_LENGTH = 8_000;
const MAX_HISTORY_MESSAGES = 20;
const TITLE_PREVIEW_LENGTH = 48;

const llmConfigRow = {
  owner: t.identity().primaryKey(),
  provider: t.string(),
  apiKey: t.string(),
  model: t.string(),
  systemPrompt: t.string().optional(),
  updatedAt: t.timestamp(),
};

const chatRow = {
  id: t.u64().primaryKey().autoInc(),
  owner: t.identity().index('btree'),
  title: t.string(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

const messageRow = {
  id: t.u64().primaryKey().autoInc(),
  chatId: t.u64().index('btree'),
  owner: t.identity().index('btree'),
  role: t.string(),
  content: t.string(),
  isError: t.bool(),
  createdAt: t.timestamp(),
};

const llmConfig = table({ name: 'llm_config', public: false }, llmConfigRow);
const chatThread = table({ name: 'chat_thread', public: false }, chatRow);
const chatMessage = table({ name: 'chat_message', public: false }, messageRow);

const spacetimedb = schema({
  llmConfig,
  chatThread,
  chatMessage,
});
export default spacetimedb;

type ModuleCtx = ReducerCtx<typeof spacetimedb.schemaType>;

function senderError(message: string): never {
  throw new SenderError(message);
}

function validateProvider(provider: string): void {
  if (!Object.prototype.hasOwnProperty.call(providers, provider)) {
    senderError(`llm.unknown_provider:${provider}`);
  }
}

function validateConfig(provider: string, model: string): void {
  validateProvider(provider);
  if (model.trim().length === 0) senderError('llm.model_required');
}

function resolveApiKey(
  existingProvider: string | undefined,
  existingApiKey: string | undefined,
  provider: string,
  apiKey: string | undefined
): string {
  const nextApiKey = apiKey?.trim();
  if (nextApiKey && nextApiKey.length > 0) return nextApiKey;
  if (
    existingProvider === provider &&
    existingApiKey &&
    existingApiKey.length > 0
  ) {
    return existingApiKey;
  }
  senderError('llm.api_key_required');
}

function validateMessage(content: string): void {
  if (content.trim().length === 0) senderError('llm.message_required');
  if (content.length > MAX_USER_MESSAGE_LENGTH) {
    senderError(`llm.message_too_long:${MAX_USER_MESSAGE_LENGTH}`);
  }
}

function makeTitle(content: string): string {
  const compact = content.trim().replace(/\s+/g, ' ');
  if (compact.length <= TITLE_PREVIEW_LENGTH) return compact;
  return `${compact.slice(0, TITLE_PREVIEW_LENGTH - 3)}...`;
}

function requireOwnedChat(ctx: ModuleCtx, chatId: bigint) {
  const chat = ctx.db.chatThread.id.find(chatId);
  if (!chat) senderError(`llm.chat_not_found:${chatId}`);
  if (!chat.owner.isEqual(ctx.sender)) senderError(`llm.not_chat_owner:${chatId}`);
  return chat;
}

function buildHistory(ctx: ModuleCtx, chatId: bigint, systemPrompt: string | undefined): ChatMessage[] {
  const rows = [...ctx.db.chatMessage.iter()]
    .filter(row => row.chatId === chatId)
    .filter(row => !row.isError && (row.role === 'user' || row.role === 'assistant'))
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    .slice(-MAX_HISTORY_MESSAGES);

  const history: ChatMessage[] = [];
  if (systemPrompt && systemPrompt.trim().length > 0) {
    history.push({ role: 'system', content: systemPrompt });
  }

  for (const row of rows) {
    history.push({
      role: row.role === 'assistant' ? 'assistant' : 'user',
      content: row.content,
    });
  }

  return history;
}

export const init = spacetimedb.init(_ctx => {});

export const onConnect = spacetimedb.clientConnected(_ctx => {});

export const onDisconnect = spacetimedb.clientDisconnected(_ctx => {});

export const chat = spacetimedb.view(
  { name: 'chat', public: true },
  t.array(t.row('ChatViewRow', chatRow)),
  ctx => ctx.from.chatThread.where(row => row.owner.eq(ctx.sender))
);

export const message = spacetimedb.view(
  { name: 'message', public: true },
  t.array(t.row('MessageViewRow', messageRow)),
  ctx => ctx.from.chatMessage.where(row => row.owner.eq(ctx.sender))
);

export const set_llm_config = spacetimedb.reducer(
  {
    provider: t.string(),
    apiKey: t.string().optional(),
    model: t.string(),
    systemPrompt: t.string().optional(),
  },
  (ctx, { provider, apiKey, model, systemPrompt }) => {
    validateConfig(provider, model);

    const existing = ctx.db.llmConfig.owner.find(ctx.sender);
    const nextApiKey = resolveApiKey(existing?.provider, existing?.apiKey, provider, apiKey);

    const row = {
      owner: ctx.sender,
      provider,
      apiKey: nextApiKey,
      model,
      systemPrompt,
      updatedAt: ctx.timestamp,
    };

    if (existing) {
      ctx.db.llmConfig.owner.update(row);
    } else {
      ctx.db.llmConfig.insert(row);
    }
  }
);

export const get_llm_config_status = spacetimedb.procedure(
  {},
  t.object('LlmConfigStatus', {
    configured: t.bool(),
    provider: t.string().optional(),
    model: t.string().optional(),
    systemPrompt: t.string().optional(),
  }),
  ctx => ctx.withTx(tx => {
    const config = tx.db.llmConfig.owner.find(tx.sender);
    return {
      configured: config != null,
      provider: config?.provider,
      model: config?.model,
      systemPrompt: config?.systemPrompt,
    };
  })
);

export const create_chat = spacetimedb.procedure(
  {},
  t.u64(),
  ctx => ctx.withTx(tx => {
    const row = tx.db.chatThread.insert({
      id: 0n,
      owner: tx.sender,
      title: 'New chat',
      createdAt: tx.timestamp,
      updatedAt: tx.timestamp,
    });
    return row.id;
  })
);

export const delete_chat = spacetimedb.reducer(
  { chatId: t.u64() },
  (ctx, { chatId }) => {
    const chat = requireOwnedChat(ctx, chatId);

    for (const message of [...ctx.db.chatMessage.iter()].filter(row => row.chatId === chatId)) {
      ctx.db.chatMessage.delete(message);
    }
    ctx.db.chatThread.delete(chat);
  }
);

export const send_message = spacetimedb.procedure(
  { chatId: t.u64(), content: t.string() },
  t.unit(),
  (ctx, { chatId, content }) => {
    validateMessage(content);

    const setup = ctx.withTx(tx => {
      const config = tx.db.llmConfig.owner.find(tx.sender);
      if (!config) senderError('llm.not_configured');
      validateProvider(config.provider);
      const chat = requireOwnedChat(tx, chatId);

      tx.db.chatMessage.insert({
        id: 0n,
        chatId,
        owner: tx.sender,
        role: 'user',
        content,
        isError: false,
        createdAt: tx.timestamp,
      });

      if (chat.title === 'New chat') {
        tx.db.chatThread.id.update({
          ...chat,
          title: makeTitle(content),
          updatedAt: tx.timestamp,
        });
      } else {
        tx.db.chatThread.id.update({ ...chat, updatedAt: tx.timestamp });
      }

      return {
        provider: config.provider,
        apiKey: config.apiKey,
        model: config.model,
        messages: buildHistory(tx, chatId, config.systemPrompt),
      };
    });

    const provider = providers[setup.provider];
    if (!provider) senderError(`llm.unknown_provider:${setup.provider}`);

    const result = callChat(ctx.http, provider, {
      apiKey: setup.apiKey,
      model: setup.model,
      messages: setup.messages,
    });

    ctx.withTx(tx => {
      tx.db.chatMessage.insert({
        id: 0n,
        chatId,
        owner: tx.sender,
        role: 'assistant',
        content: result.ok ? result.response.text : formatChatError(result.error),
        isError: !result.ok,
        createdAt: tx.timestamp,
      });

      const chat = tx.db.chatThread.id.find(chatId);
      if (chat) tx.db.chatThread.id.update({ ...chat, updatedAt: tx.timestamp });
    });

    return {};
  }
);
