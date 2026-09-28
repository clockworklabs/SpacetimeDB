import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import type { Infer } from 'spacetimedb';
import { procedures, reducers, tables } from './module_bindings';
import ChatRow from './module_bindings/chat_table';
import MessageRow from './module_bindings/message_table';
import {
  useProcedure,
  useReducer,
  useSpacetimeDB,
  useTable,
} from 'spacetimedb/react';

type Chat = Infer<typeof ChatRow>;
type Message = Infer<typeof MessageRow>;

type ConfigStatus = {
  configured: boolean;
  provider?: string;
  model?: string;
  systemPrompt?: string;
};

type ConfigDraft = {
  provider: string;
  apiKey: string;
  model: string;
  systemPrompt: string;
};

const DEFAULT_OPENROUTER_MODEL = 'openai/gpt-4o-mini';
const DEFAULT_OPENAI_MODEL = 'gpt-4o-mini';

function defaultModel(provider: string) {
  return provider === 'openai' ? DEFAULT_OPENAI_MODEL : DEFAULT_OPENROUTER_MODEL;
}

function chatUpdatedMicros(chat: Chat): bigint {
  return chat.updatedAt.microsSinceUnixEpoch as bigint;
}

function sortChats(chats: readonly Chat[]) {
  return [...chats].sort((a, b) => {
    const av = chatUpdatedMicros(a);
    const bv = chatUpdatedMicros(b);
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
}

function sortMessages(messages: Message[]) {
  return [...messages].sort((a, b) => {
    if (a.id === b.id) return 0;
    return a.id < b.id ? -1 : 1;
  });
}

type MessagePart =
  | { kind: 'text'; text: string }
  | { kind: 'code'; code: string; language: string };

type InlinePart =
  | { kind: 'text'; text: string }
  | { kind: 'bold'; text: string }
  | { kind: 'inlineCode'; text: string };

function parseMessageContent(content: string): MessagePart[] {
  const parts: MessagePart[] = [];
  const fencePattern = /```([^\n`]*)\n?([\s\S]*?)```/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = fencePattern.exec(content)) != null) {
    if (match.index > cursor) {
      parts.push({ kind: 'text', text: content.slice(cursor, match.index) });
    }
    parts.push({
      kind: 'code',
      language: match[1].trim(),
      code: match[2].replace(/\n$/, ''),
    });
    cursor = match.index + match[0].length;
  }

  if (cursor < content.length) {
    parts.push({ kind: 'text', text: content.slice(cursor) });
  }

  return parts.length === 0 ? [{ kind: 'text', text: content }] : parts;
}

function renderInlineText(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const inlinePattern = /(`[^`\n]+`|\*\*[^*\n][\s\S]*?\*\*)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = inlinePattern.exec(text)) != null) {
    if (match.index > cursor) {
      nodes.push(text.slice(cursor, match.index));
    }

    const token = match[0];
    const part: InlinePart = token.startsWith('`')
      ? { kind: 'inlineCode', text: token.slice(1, -1) }
      : { kind: 'bold', text: token.slice(2, -2) };

    nodes.push(
      part.kind === 'inlineCode' ? (
        <code className="inline-code" key={`${match.index}-code`}>
          {part.text}
        </code>
      ) : (
        <strong key={`${match.index}-bold`}>{part.text}</strong>
      )
    );
    cursor = match.index + token.length;
  }

  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

function MessageContent({ content }: { content: string }) {
  return (
    <div className="message-content">
      {parseMessageContent(content).map((part, index) => {
        if (part.kind === 'code') {
          return (
            <div className="code-block" key={index}>
              {part.language && <div className="code-label">{part.language}</div>}
              <pre>
                <code>{part.code}</code>
              </pre>
            </div>
          );
        }

        return part.text
          .split(/\n{2,}/)
          .filter(paragraph => paragraph.length > 0)
          .map((paragraph, paragraphIndex) => (
            <p key={`${index}-${paragraphIndex}`}>
              {renderInlineText(paragraph)}
            </p>
          ));
      })}
    </div>
  );
}

function App() {
  const { isActive: connected } = useSpacetimeDB();
  const [chats] = useTable(tables.chat);
  const [messages] = useTable(tables.message);

  const createChat = useProcedure(procedures.createChat);
  const getConfigStatus = useProcedure(procedures.getLlmConfigStatus);
  const sendMessage = useProcedure(procedures.sendMessage);
  const setConfig = useReducer(reducers.setLlmConfig);
  const deleteChatReducer = useReducer(reducers.deleteChat);

  const [activeChatId, setActiveChatId] = useState<bigint | null>(null);
  const [configStatus, setConfigStatus] = useState<ConfigStatus>({
    configured: false,
  });
  const [configOpen, setConfigOpen] = useState(false);
  const [configDraft, setConfigDraft] = useState<ConfigDraft>({
    provider: 'openrouter',
    apiKey: '',
    model: DEFAULT_OPENROUTER_MODEL,
    systemPrompt: 'You are a concise, helpful assistant.',
  });
  const [modelEdited, setModelEdited] = useState(false);
  const [composerText, setComposerText] = useState('');
  const [statusText, setStatusText] = useState('');
  const [sending, setSending] = useState(false);
  const messageEndRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const focusComposerAfterSend = useRef(false);

  const sortedChats = useMemo(() => sortChats(chats), [chats]);
  const activeChat = sortedChats.find(chat => chat.id === activeChatId);
  const activeMessages = useMemo(
    () =>
      sortMessages(
        activeChatId == null
          ? []
          : messages.filter(message => message.chatId === activeChatId)
      ),
    [activeChatId, messages]
  );

  useEffect(() => {
    if (!connected) return;
    getConfigStatus()
      .then(status => {
        setConfigStatus(status);
        setConfigDraft(draft => ({
          ...draft,
          provider: status.provider ?? draft.provider,
          model: status.model ?? draft.model,
          systemPrompt: status.systemPrompt ?? draft.systemPrompt,
        }));
        if (!status.configured) setConfigOpen(true);
      })
      .catch(err => {
        setStatusText(err instanceof Error ? err.message : String(err));
      });
  }, [connected, getConfigStatus]);

  useEffect(() => {
    if (activeChatId == null && sortedChats.length > 0) {
      setActiveChatId(sortedChats[0].id);
      return;
    }
    if (
      activeChatId != null &&
      !sortedChats.some(chat => chat.id === activeChatId)
    ) {
      setActiveChatId(sortedChats[0]?.id ?? null);
    }
  }, [activeChatId, sortedChats]);

  useEffect(() => {
    messageEndRef.current?.scrollIntoView({ block: 'end' });
  }, [activeMessages.length, sending]);

  useEffect(() => {
    if (sending || !focusComposerAfterSend.current) return;
    focusComposerAfterSend.current = false;
    composerRef.current?.focus();
  }, [sending]);

  const updateProvider = (provider: string) => {
    setConfigDraft(draft => ({
      ...draft,
      provider,
      model: modelEdited ? draft.model : defaultModel(provider),
    }));
  };

  const onNewChat = async () => {
    if (!connected) return;
    setStatusText('');
    try {
      const chatId = await createChat();
      setActiveChatId(chatId);
    } catch (err) {
      setStatusText(err instanceof Error ? err.message : String(err));
    }
  };

  const onDeleteChat = async (chatId: bigint) => {
    setStatusText('');
    try {
      await deleteChatReducer({ chatId });
      if (activeChatId === chatId) setActiveChatId(null);
    } catch (err) {
      setStatusText(err instanceof Error ? err.message : String(err));
    }
  };

  const onSaveConfig = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const providerChanged =
      configStatus.configured && configStatus.provider !== configDraft.provider;
    if (providerChanged && configDraft.apiKey.trim().length === 0) {
      setStatusText('Enter an API key when switching providers.');
      return;
    }

    setStatusText('Saving config...');
    try {
      await setConfig({
        provider: configDraft.provider,
        apiKey: configDraft.apiKey.trim() || undefined,
        model: configDraft.model.trim(),
        systemPrompt: configDraft.systemPrompt.trim() || undefined,
      });
      const status = await getConfigStatus();
      setConfigStatus(status);
      setConfigDraft(draft => ({ ...draft, apiKey: '' }));
      setConfigOpen(false);
      setStatusText('Config saved.');
    } catch (err) {
      setStatusText(err instanceof Error ? err.message : String(err));
    }
  };

  const onSend = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!connected || sending) return;
    if (!configStatus.configured) {
      setConfigOpen(true);
      return;
    }

    const content = composerText.trim();
    if (!content) return;

    setSending(true);
    setStatusText('');
    setComposerText('');

    try {
      const chatId = activeChatId ?? (await createChat());
      if (activeChatId == null) setActiveChatId(chatId);
      await sendMessage({ chatId, content });
    } catch (err) {
      setComposerText(content);
      setStatusText(err instanceof Error ? err.message : String(err));
    } finally {
      focusComposerAfterSend.current = true;
      setSending(false);
    }
  };

  const onComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    event.currentTarget.form?.requestSubmit();
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1>LLM Chat</h1>
          <span className={connected ? 'status online' : 'status offline'}>
            {connected ? 'Connected' : 'Disconnected'}
          </span>
        </div>

        <button className="new-chat" onClick={onNewChat} disabled={!connected}>
          New chat
        </button>

        <nav className="chat-list" aria-label="Chats">
          {sortedChats.length === 0 ? (
            <p className="muted">No chats yet.</p>
          ) : (
            sortedChats.map(chat => (
              <div
                key={chat.id.toString()}
                className={chat.id === activeChatId ? 'chat-row active' : 'chat-row'}
              >
                <button
                  type="button"
                  className="chat-select"
                  onClick={() => setActiveChatId(chat.id)}
                >
                  <span>{chat.title}</span>
                </button>
                <button
                  type="button"
                  className="delete-chat"
                  onClick={() => void onDeleteChat(chat.id)}
                  aria-label={`Delete ${chat.title}`}
                >
                  ×
                </button>
              </div>
            ))
          )}
        </nav>

        <div className="sidebar-footer">
          <button onClick={() => setConfigOpen(true)} disabled={!connected}>
            {configStatus.configured ? 'Edit config' : 'Set up provider'}
          </button>
          {configStatus.configured && (
            <p className="muted">
              {configStatus.provider} · {configStatus.model}
            </p>
          )}
        </div>
      </aside>

      <main className="chat-pane">
        <header className="chat-header">
          <div>
            <h2>{activeChat?.title ?? 'New chat'}</h2>
            <p>Each chat has its own context. Your config is private to this identity.</p>
          </div>
        </header>

        <section className="messages" aria-live="polite">
          {activeMessages.length === 0 ? (
            <div className="empty-state">
              <h3>Start a clean chat</h3>
              <p>Ask a question and the module will call your configured model.</p>
            </div>
          ) : (
            activeMessages.map(message => (
              <article
                key={message.id.toString()}
                className={`message ${message.role} ${message.isError ? 'error' : ''}`}
              >
                <div className="message-role">
                  {message.role === 'assistant' ? 'Assistant' : 'You'}
                </div>
                <MessageContent content={message.content} />
                <time>
                  {message.createdAt.toDate().toLocaleTimeString([], {
                    hour: '2-digit',
                    minute: '2-digit',
                  })}
                </time>
              </article>
            ))
          )}
          {sending && <div className="thinking">Assistant is thinking...</div>}
          <div ref={messageEndRef} />
        </section>

        <form className="composer" onSubmit={onSend}>
          {statusText && <div className="status-line">{statusText}</div>}
          <div className="composer-row">
            <textarea
              ref={composerRef}
              value={composerText}
              onChange={event => setComposerText(event.target.value)}
              onKeyDown={onComposerKeyDown}
              placeholder={
                configStatus.configured
                  ? 'Message the assistant...'
                  : 'Configure a provider before sending...'
              }
              disabled={!connected || sending}
              rows={3}
            />
            <button type="submit" disabled={!connected || sending}>
              Send
            </button>
          </div>
        </form>
      </main>

      {configOpen && (
        <div className="modal-backdrop" role="presentation">
          <form className="config-modal" onSubmit={onSaveConfig}>
            <header>
              <h2>Provider config</h2>
              <button type="button" onClick={() => setConfigOpen(false)}>
                Close
              </button>
            </header>

            <label>
              Provider
              <select
                value={configDraft.provider}
                onChange={event => updateProvider(event.target.value)}
              >
                <option value="openrouter">OpenRouter</option>
                <option value="openai">OpenAI</option>
              </select>
            </label>

            <label>
              API key
              <input
                type="password"
                value={configDraft.apiKey}
                onChange={event =>
                  setConfigDraft(draft => ({
                    ...draft,
                    apiKey: event.target.value,
                  }))
                }
                placeholder={
                  configStatus.configured &&
                  configStatus.provider === configDraft.provider
                    ? 'Leave blank to keep saved key'
                    : 'API key'
                }
                autoComplete="off"
              />
            </label>

            <label>
              Model
              <input
                value={configDraft.model}
                onChange={event => {
                  setModelEdited(true);
                  setConfigDraft(draft => ({
                    ...draft,
                    model: event.target.value,
                  }));
                }}
              />
            </label>

            <label>
              System prompt
              <textarea
                value={configDraft.systemPrompt}
                onChange={event =>
                  setConfigDraft(draft => ({
                    ...draft,
                    systemPrompt: event.target.value,
                  }))
                }
                rows={4}
              />
            </label>

            <footer>
              <button type="submit" disabled={!connected}>
                Save config
              </button>
            </footer>
          </form>
        </div>
      )}
    </div>
  );
}

export default App;
