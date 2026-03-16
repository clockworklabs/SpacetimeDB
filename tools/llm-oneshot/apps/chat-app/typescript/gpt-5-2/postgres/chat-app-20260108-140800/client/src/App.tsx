import React, { useEffect, useMemo, useRef, useState } from 'react';
import type { Socket } from 'socket.io-client';
import {
  cancelScheduled,
  createRoom,
  editMessage,
  getEditHistory,
  getMe,
  getStoredToken,
  joinRoom,
  leaveRoom,
  listMembers,
  listMessages,
  listRooms,
  listScheduled,
  login,
  markRead,
  scheduleMessage,
  sendEphemeral,
  sendMessage,
  toggleReaction,
} from './api';
import { connectSocket } from './socket';
import type { Member, Message, Room, ScheduledMessage, User } from './types';

type ComposerMode = 'normal' | 'ephemeral';

function formatRoomName(name: string) {
  return name.startsWith('#') ? name : `#${name}`;
}

function formatWhen(date: Date) {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function secondsLeft(expiresAt: string, nowMs: number): number {
  const t = new Date(expiresAt).getTime();
  return Math.max(0, Math.ceil((t - nowMs) / 1000));
}

export function App() {
  const [me, setMe] = useState<User | null>(null);
  const [rooms, setRooms] = useState<Room[]>([]);
  const [activeRoomId, setActiveRoomId] = useState<number | null>(null);
  const [messagesState, setMessagesState] = useState<Message[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [scheduled, setScheduled] = useState<ScheduledMessage[]>([]);
  const [loadingRooms, setLoadingRooms] = useState(false);
  const [loadingRoom, setLoadingRoom] = useState(false);

  const [loginName, setLoginName] = useState('');

  const [roomName, setRoomName] = useState('');

  const [draft, setDraft] = useState('');
  const [composerMode, setComposerMode] = useState<ComposerMode>('normal');
  const [ephemeralSeconds, setEphemeralSeconds] = useState(60);
  const [showSchedulePanel, setShowSchedulePanel] = useState(false);
  const [scheduleInSeconds, setScheduleInSeconds] = useState(30);
  const [scheduleAtLocal, setScheduleAtLocal] = useState('');

  const [unreadByRoomId, setUnreadByRoomId] = useState<Record<number, number>>(
    {}
  );

  const [typingUsers, setTypingUsers] = useState<
    { userId: string; displayName: string }[]
  >([]);

  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editingDraft, setEditingDraft] = useState('');

  const [historyOpenFor, setHistoryOpenFor] = useState<number | null>(null);
  const [historyVersions, setHistoryVersions] = useState<
    { label: string; content: string }[]
  >([]);

  const [nowMs, setNowMs] = useState(() => Date.now());

  const socketRef = useRef<Socket | null>(null);
  const typingDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const activeRoom = useMemo(
    () => rooms.find(r => r.id === activeRoomId) || null,
    [rooms, activeRoomId]
  );

  async function refreshRooms() {
    setLoadingRooms(true);
    try {
      const next = await listRooms();
      setRooms(next);
      if (activeRoomId === null && next.length > 0) setActiveRoomId(next[0].id);
    } finally {
      setLoadingRooms(false);
    }
  }

  async function refreshActiveRoom() {
    if (!activeRoomId) return;
    setLoadingRoom(true);
    try {
      const [msgs, mems] = await Promise.all([
        listMessages(activeRoomId),
        listMembers(activeRoomId),
      ]);
      setMessagesState(msgs);
      setMembers(mems);

      const lastId = msgs.length ? msgs[msgs.length - 1].id : null;
      await markRead(activeRoomId, lastId);
      setUnreadByRoomId(prev => ({ ...prev, [activeRoomId]: 0 }));
    } finally {
      setLoadingRoom(false);
    }
  }

  async function refreshScheduled() {
    try {
      const rows = await listScheduled();
      setScheduled(rows);
    } catch {
      setScheduled([]);
    }
  }

  useEffect(() => {
    const interval = setInterval(() => setNowMs(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  // Bootstrap auth + socket
  useEffect(() => {
    let mounted = true;
    (async () => {
      const user = await getMe();
      if (!mounted) return;
      setMe(user);
      if (!user) return;

      await refreshRooms();
      await refreshScheduled();

      const token = getStoredToken();
      if (!token) return;
      const socket = connectSocket(token);
      socketRef.current = socket;

      socket.on('rooms:changed', () => void refreshRooms());
      socket.on('roomMembers:changed', (p: any) => {
        if (Number(p?.roomId) === activeRoomId) void refreshActiveRoom();
      });
      socket.on('message:created', (p: any) => {
        const roomId = Number(p?.roomId);
        if (!Number.isFinite(roomId)) return;
        if (roomId === activeRoomId) void refreshActiveRoom();
        else
          setUnreadByRoomId(prev => ({
            ...prev,
            [roomId]: (prev[roomId] || 0) + 1,
          }));
      });
      socket.on('message:updated', (p: any) => {
        if (Number(p?.roomId) === activeRoomId) void refreshActiveRoom();
      });
      socket.on('message:deleted', (p: any) => {
        if (Number(p?.roomId) === activeRoomId) void refreshActiveRoom();
      });
      socket.on('reactions:changed', () => {
        if (activeRoomId) void refreshActiveRoom();
      });
      socket.on('reads:changed', (p: any) => {
        if (Number(p?.roomId) === activeRoomId) void refreshActiveRoom();
      });
      socket.on('presence:changed', () => {
        if (activeRoomId) void refreshActiveRoom();
      });
      socket.on('typing:state', (p: any) => {
        if (Number(p?.roomId) !== activeRoomId) return;
        setTypingUsers(Array.isArray(p?.users) ? p.users : []);
      });
      socket.on('scheduled:changed', () => void refreshScheduled());
    })();
    return () => {
      mounted = false;
      socketRef.current?.disconnect();
      socketRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!me) return;
    void refreshActiveRoom();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeRoomId, me?.id]);

  function startTyping(roomId: number) {
    const socket = socketRef.current;
    if (!socket) return;
    socket.emit('typing:start', { roomId });
    if (typingDebounceRef.current) clearTimeout(typingDebounceRef.current);
    typingDebounceRef.current = setTimeout(
      () => socket.emit('typing:stop', { roomId }),
      2500
    );
  }

  async function handleLoginSubmit(e: React.FormEvent) {
    e.preventDefault();
    const name = loginName.trim();
    if (!name) return;
    const user = await login(name);
    setMe(user);
    await refreshRooms();
    await refreshScheduled();

    const token = getStoredToken();
    if (token) {
      const socket = connectSocket(token);
      socketRef.current = socket;
      socket.on('rooms:changed', () => void refreshRooms());
      socket.on('scheduled:changed', () => void refreshScheduled());
    }
  }

  async function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    const name = roomName.trim();
    if (!name) return;
    const room = await createRoom(name);
    setRoomName('');
    setRooms(prev => [room, ...prev]);
    setActiveRoomId(room.id);
  }

  async function handleSelectRoom(roomId: number) {
    setActiveRoomId(roomId);
    await joinRoom(roomId);
    setUnreadByRoomId(prev => ({ ...prev, [roomId]: 0 }));
  }

  async function handleSend(e: React.FormEvent) {
    e.preventDefault();
    if (!activeRoomId) return;
    const content = draft.trim();
    if (!content) return;

    if (composerMode === 'ephemeral') {
      await sendEphemeral(activeRoomId, content, ephemeralSeconds);
    } else {
      await sendMessage(activeRoomId, content);
    }
    setDraft('');
    socketRef.current?.emit('typing:stop', { roomId: activeRoomId });
  }

  async function handleScheduleClick() {
    if (!activeRoomId) return;
    const content = draft.trim();
    setShowSchedulePanel(true);
    if (!content) return;

    // If user filled a datetime-local input, prefer it.
    let seconds = scheduleInSeconds;
    if (scheduleAtLocal) {
      const t = new Date(scheduleAtLocal).getTime();
      if (Number.isFinite(t))
        seconds = Math.max(5, Math.ceil((t - Date.now()) / 1000));
    }
    await scheduleMessage(activeRoomId, content, seconds);
    setDraft('');
    await refreshScheduled();
  }

  async function handleCancelScheduled(id: number) {
    await cancelScheduled(id);
    await refreshScheduled();
  }

  async function beginEdit(msg: Message) {
    setShowSchedulePanel(false);
    setEditingMessageId(msg.id);
    setEditingDraft(msg.content);
  }

  async function commitEdit(messageId: number) {
    const next = editingDraft.trim();
    if (!next) return;
    await editMessage(messageId, next);
    setEditingMessageId(null);
    setEditingDraft('');
    await refreshActiveRoom();
  }

  async function openHistory(messageId: number) {
    setHistoryOpenFor(messageId);
    const versions = await getEditHistory(messageId);
    setHistoryVersions(versions);
  }

  const onlineMembers = useMemo(
    () => members.filter(m => m.isOnline),
    [members]
  );

  const typingText = useMemo(() => {
    const others = typingUsers.filter(u => u.userId !== me?.id);
    if (!others.length) return '';
    if (others.length === 1) return `${others[0].displayName} is typing...`;
    if (others.length === 2)
      return `${others[0].displayName} and ${others[1].displayName} are typing...`;
    return `Multiple users are typing...`;
  }, [typingUsers, me?.id]);

  function seenByForMessage(msg: Message): string[] {
    const seen = members
      .filter(m => m.id !== msg.authorId)
      .filter(m => (m.lastReadMessageId ?? 0) >= msg.id)
      .map(m => m.displayName);
    return seen;
  }

  if (!me) {
    return (
      <div className="login">
        <div className="card">
          <h1>Chat App</h1>
          <p className="muted">Pick a display name to get started.</p>
          <form onSubmit={handleLoginSubmit} className="row">
            <input
              name="displayName"
              placeholder="Display name"
              value={loginName}
              onChange={e => setLoginName(e.target.value)}
              autoFocus
            />
            <button type="submit">Join</button>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className={`app ${editingMessageId ? 'editing' : ''}`}>
      <aside className="sidebar">
        <div className="me">
          <div className="me-name">{me.displayName}</div>
          <div className="me-sub muted">Online</div>
        </div>

        <form className="create-room" onSubmit={handleCreateRoom}>
          <input
            placeholder="Room name"
            name="roomName"
            value={roomName}
            onChange={e => setRoomName(e.target.value)}
            className="small"
          />
          <button type="submit" data-testid="create-room" className="small">
            Create
          </button>
        </form>

        <div className="section-title">Rooms</div>
        {loadingRooms ? (
          <div className="muted pad">Loading rooms…</div>
        ) : rooms.length === 0 ? (
          <div className="muted pad">No rooms yet. Create one.</div>
        ) : (
          <div className="room-list">
            {rooms.map(r => {
              const unread = unreadByRoomId[r.id] || 0;
              const active = r.id === activeRoomId;
              return (
                <button
                  key={r.id}
                  className={`room ${active ? 'active' : ''}`}
                  onClick={() => void handleSelectRoom(r.id)}
                  type="button"
                >
                  <span className="room-name">{formatRoomName(r.name)}</span>
                  {unread > 0 ? (
                    <span className="badge unread-count">({unread})</span>
                  ) : null}
                </button>
              );
            })}
          </div>
        )}

        <div className="section-title">Scheduled</div>
        {scheduled.length === 0 ? (
          <div className="muted pad">No pending messages.</div>
        ) : (
          <div className="scheduled-list">
            {scheduled.map(s => (
              <div key={s.id} className="scheduled-item">
                <div className="scheduled-meta">
                  <div className="scheduled-room">
                    {formatRoomName(s.roomName)}
                  </div>
                  <div className="muted">{formatWhen(new Date(s.sendAt))}</div>
                </div>
                <div className="scheduled-content">{s.content}</div>
                <button
                  className="danger small"
                  onClick={() => void handleCancelScheduled(s.id)}
                  type="button"
                >
                  Cancel
                </button>
              </div>
            ))}
          </div>
        )}
      </aside>

      <main className="chat">
        <header className="chat-header">
          <div className="chat-title">
            {activeRoom ? formatRoomName(activeRoom.name) : 'Select a room'}
          </div>
          <div className="chat-actions">
            <div className="muted">{onlineMembers.length} online</div>
            {activeRoomId ? (
              <button
                type="button"
                className="small"
                onClick={() => {
                  void (async () => {
                    await leaveRoom(activeRoomId);
                    setActiveRoomId(null);
                    await refreshRooms();
                  })();
                }}
              >
                Leave
              </button>
            ) : null}
          </div>
        </header>

        <div className="chat-body">
          {loadingRoom ? (
            <div className="muted pad">Loading messages…</div>
          ) : !activeRoomId ? (
            <div className="muted pad">Create or select a room.</div>
          ) : messagesState.length === 0 ? (
            <div className="muted pad">No messages yet. Say hi!</div>
          ) : (
            <div className="messages">
              {messagesState.map(m => {
                const isMine = m.authorId === me.id;
                const seenBy = seenByForMessage(m);
                const exp = m.expiresAt
                  ? secondsLeft(m.expiresAt, nowMs)
                  : null;
                const reactionThumbs = m.reactions.find(r => r.emoji === '👍');
                const thumbCount = reactionThumbs
                  ? reactionThumbs.userIds.length
                  : 0;
                const thumbTitle = reactionThumbs
                  ? reactionThumbs.userIds
                      .map(
                        id =>
                          members.find(mm => mm.id === id)?.displayName ||
                          id.slice(0, 6)
                      )
                      .join(', ')
                  : '';

                return (
                  <div key={m.id} className="msg">
                    <div className="msg-main">
                      <div className="msg-top">
                        <span className="msg-author">{m.authorName}</span>
                        <span className="msg-time muted">
                          {formatWhen(new Date(m.createdAt))}
                        </span>
                        {m.edited ? (
                          <button
                            type="button"
                            className="link edited"
                            onClick={() => void openHistory(m.id)}
                            data-testid="history"
                          >
                            (edited)
                          </button>
                        ) : null}
                        {exp !== null ? (
                          <span className="countdown" data-testid="countdown">
                            {exp}s
                          </span>
                        ) : null}
                      </div>

                      {editingMessageId === m.id ? (
                        <textarea
                          className="edit-input"
                          value={editingDraft}
                          onChange={e => setEditingDraft(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Enter' && !e.shiftKey) {
                              e.preventDefault();
                              void commitEdit(m.id);
                            }
                          }}
                        />
                      ) : (
                        <div className="msg-content">{m.content}</div>
                      )}

                      <div className="msg-meta">
                        {seenBy.length ? (
                          <div className="read-receipt">
                            Seen by {seenBy.slice(0, 4).join(', ')}
                            {seenBy.length > 4 ? ` +${seenBy.length - 4}` : ''}
                          </div>
                        ) : null}
                        {thumbCount > 0 ? (
                          <button
                            type="button"
                            className="reaction-pill"
                            title={thumbTitle}
                            onClick={() => void toggleReaction(m.id, '👍')}
                          >
                            👍 {thumbCount}
                          </button>
                        ) : null}
                      </div>
                    </div>

                    <div className="msg-actions">
                      <button
                        type="button"
                        className="icon reaction-btn"
                        data-testid="reaction"
                        title="React"
                        onClick={() => void toggleReaction(m.id, '👍')}
                      >
                        👍
                      </button>
                      {isMine ? (
                        <button
                          type="button"
                          className="icon"
                          data-testid="edit"
                          title="Edit"
                          onClick={() => void beginEdit(m)}
                        >
                          Edit
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="typing-indicator" data-testid="typing">
          {typingText}
        </div>

        <form className="composer" onSubmit={handleSend}>
          <div className="composer-row">
            <textarea
              placeholder="Message"
              name="message"
              value={draft}
              onChange={e => {
                setDraft(e.target.value);
                if (activeRoomId) startTyping(activeRoomId);
              }}
              onBlur={() => {
                if (activeRoomId)
                  socketRef.current?.emit('typing:stop', {
                    roomId: activeRoomId,
                  });
              }}
            />
            <button type="submit" data-testid="send-button">
              Send
            </button>
          </div>

          <div className="composer-tools">
            <button
              type="button"
              className={`pill ${composerMode === 'ephemeral' ? 'active' : ''}`}
              data-testid="ephemeral"
              onClick={() =>
                setComposerMode(m =>
                  m === 'ephemeral' ? 'normal' : 'ephemeral'
                )
              }
              title="Disappear"
            >
              Disappear
            </button>

            {composerMode === 'ephemeral' ? (
              <select
                value={String(ephemeralSeconds)}
                onChange={e => setEphemeralSeconds(Number(e.target.value))}
                aria-label="Ephemeral duration"
              >
                <option value="60">1 minute</option>
                <option value="300">5 minutes</option>
              </select>
            ) : null}

            <button
              type="button"
              className="pill"
              data-testid="schedule"
              onClick={() => void handleScheduleClick()}
            >
              Schedule
            </button>

            {showSchedulePanel ? (
              <div className="schedule-panel">
                <div className="schedule-row">
                  <label className="muted">In</label>
                  <select
                    value={String(scheduleInSeconds)}
                    onChange={e => setScheduleInSeconds(Number(e.target.value))}
                  >
                    <option value="30">30s</option>
                    <option value="60">1m</option>
                  </select>
                </div>
                <div className="schedule-row">
                  <label className="muted">Or pick time</label>
                  <input
                    type="datetime-local"
                    value={scheduleAtLocal}
                    onChange={e => setScheduleAtLocal(e.target.value)}
                  />
                </div>
                <button
                  type="button"
                  className="small"
                  onClick={() => setShowSchedulePanel(false)}
                >
                  Close
                </button>
              </div>
            ) : null}
          </div>
        </form>
      </main>

      <aside className="members">
        <div className="section-title">Online</div>
        {onlineMembers.length === 0 ? (
          <div className="muted pad">No one online.</div>
        ) : (
          <div className="member-list">
            {onlineMembers.map(m => (
              <div key={m.id} className="member">
                <span className="dot" />
                <span>{m.displayName}</span>
              </div>
            ))}
          </div>
        )}
      </aside>

      {historyOpenFor ? (
        <div className="modal-backdrop" onClick={() => setHistoryOpenFor(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <div className="modal-title">Edit history</div>
              <button
                type="button"
                className="small"
                onClick={() => setHistoryOpenFor(null)}
              >
                Close
              </button>
            </div>
            <div className="history">
              {historyVersions.map((v, idx) => (
                <div key={idx} className="history-item">
                  <div className="muted">{v.label}</div>
                  <div className="history-content">{v.content}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
