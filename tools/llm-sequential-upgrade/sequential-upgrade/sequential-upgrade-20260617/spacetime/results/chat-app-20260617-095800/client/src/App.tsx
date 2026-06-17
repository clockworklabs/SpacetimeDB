import React, { useEffect, useRef, useState, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message } from './module_bindings/types';
import type { Identity } from 'spacetimedb';

// ── helpers ──────────────────────────────────────────────────────────────────

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  const date = new Date(Number(ts.microsSinceUnixEpoch / 1000n));
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function idHex(id: Identity): string {
  return id.toHexString();
}

function colorForName(name: string): string {
  const colors = [
    '#4cf490', '#a880ff', '#02befa', '#fbdc8e',
    '#ff4c4c', '#4cf4d4', '#f490cf', '#90c8f4',
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
  return colors[Math.abs(hash) % colors.length];
}

// ── Setup screen ─────────────────────────────────────────────────────────────

function SetupScreen({ onSetName }: { onSetName: (name: string) => void }) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) { setError('Name is required'); return; }
    if (trimmed.length > 32) { setError('Name too long'); return; }
    onSetName(trimmed);
  }

  return (
    <div className="setup-screen">
      <div className="setup-card">
        <h1 className="app-title">SpacetimeDB Chat</h1>
        <p className="setup-subtitle">Enter your display name to join</p>
        <form onSubmit={handleSubmit} className="setup-form">
          <input
            className="input"
            type="text"
            placeholder="Enter your name"
            value={name}
            onChange={e => { setName(e.target.value); setError(''); }}
            autoFocus
          />
          {error && <p className="error-text">{error}</p>}
          <button type="submit" className="btn btn-primary">
            Join
          </button>
        </form>
      </div>
    </div>
  );
}

// ── Create room modal ─────────────────────────────────────────────────────────

function CreateRoomModal({ onClose, onCreate }: {
  onClose: () => void;
  onCreate: (name: string) => void;
}) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) { setError('Room name is required'); return; }
    onCreate(trimmed);
    onClose();
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) { if (e.key === 'Escape') onClose(); }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">Create Room</h2>
        <form onSubmit={handleSubmit}>
          <input
            className="input"
            type="text"
            placeholder="Room name"
            value={name}
            onChange={e => { setName(e.target.value); setError(''); }}
            autoFocus
          />
          {error && <p className="error-text">{error}</p>}
          <div className="modal-actions">
            <button type="button" className="btn btn-ghost" onClick={onClose}>Cancel</button>
            <button type="submit" className="btn btn-primary">Create</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ── Main App ──────────────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [activeRoomId, setActiveRoomId] = useState<bigint | null>(null);
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [messageText, setMessageText] = useState('');
  const [hasSetName, setHasSetName] = useState(false);
  const [isScrolledUp, setIsScrolledUp] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.membership,
        tables.message,
        tables.typingIndicator,
        tables.readReceipt,
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [memberships] = useTable(tables.membership);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);

  const myUser = myIdentity ? users.find(u => idHex(u.identity) === idHex(myIdentity)) : undefined;
  const myMemberships = myIdentity
    ? memberships.filter(m => idHex(m.userIdentity) === idHex(myIdentity))
    : [];
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));
  const joinedRooms = rooms.filter(r => myRoomIds.has(r.id));
  const otherRooms = rooms.filter(r => !myRoomIds.has(r.id));

  const activeRoomMessages = activeRoomId
    ? messages.filter(m => m.roomId === activeRoomId).sort((a, b) => (a.sentAt.microsSinceUnixEpoch < b.sentAt.microsSinceUnixEpoch ? -1 : 1))
    : [];

  const onlineUsers = users.filter(u => u.online);

  // ── unread counts ───────────────────────────────────────────────────────────

  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const receipt = readReceipts.find(
      r => r.roomId === roomId && idHex(r.userIdentity) === idHex(myIdentity)
    );
    const roomMessages = messages.filter(m => m.roomId === roomId);
    if (!receipt) return roomMessages.length;
    return roomMessages.filter(m => m.id > receipt.lastReadMessageId).length;
  }

  // ── mark read when room is active ───────────────────────────────────────────

  useEffect(() => {
    if (!activeRoomId || !conn || !myIdentity) return;
    const roomMsgs = messages.filter(m => m.roomId === activeRoomId);
    if (roomMsgs.length === 0) return;
    const maxId = roomMsgs.reduce((acc, m) => (m.id > acc ? m.id : acc), 0n);
    const existing = readReceipts.find(
      r => r.roomId === activeRoomId && idHex(r.userIdentity) === idHex(myIdentity)
    );
    if (!existing || existing.lastReadMessageId < maxId) {
      conn.reducers.markRead({ roomId: activeRoomId, lastReadMessageId: maxId });
    }
  }, [activeRoomId, messages, readReceipts, conn, myIdentity]);

  // ── auto-scroll ─────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!isScrolledUp) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [activeRoomMessages.length, isScrolledUp]);

  function handleScroll() {
    const el = messagesContainerRef.current;
    if (!el) return;
    const threshold = 100;
    setIsScrolledUp(el.scrollHeight - el.scrollTop - el.clientHeight > threshold);
  }

  // ── typing ──────────────────────────────────────────────────────────────────

  const sendTypingStop = useCallback(() => {
    if (isTypingRef.current && conn && activeRoomId) {
      conn.reducers.setTyping({ roomId: activeRoomId, isTyping: false });
      isTypingRef.current = false;
    }
  }, [conn, activeRoomId]);

  function handleMessageInput(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageText(e.target.value);
    if (!conn || !activeRoomId) return;
    if (!isTypingRef.current) {
      conn.reducers.setTyping({ roomId: activeRoomId, isTyping: true });
      isTypingRef.current = true;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      sendTypingStop();
    }, 4000);
  }

  // ── typing expiry (client-side for display) ─────────────────────────────────
  // The server stores timestamps; filter indicators older than 5 seconds

  const now = Date.now();
  const activeTypers = activeRoomId
    ? typingIndicators.filter(ti => {
        if (ti.roomId !== activeRoomId) return false;
        if (myIdentity && idHex(ti.userIdentity) === idHex(myIdentity)) return false;
        const age = now - Number(ti.updatedAt.microsSinceUnixEpoch / 1000n);
        return age < 5000;
      })
    : [];

  // ── read receipts display ───────────────────────────────────────────────────

  function getReadersForMessage(msg: Message): string[] {
    if (!myIdentity) return [];
    return readReceipts
      .filter(r => r.roomId === msg.roomId && r.lastReadMessageId >= msg.id && idHex(r.userIdentity) !== idHex(myIdentity))
      .map(r => {
        const user = users.find(u => idHex(u.identity) === idHex(r.userIdentity));
        return user?.name ?? '???';
      });
  }

  // ── actions ─────────────────────────────────────────────────────────────────

  function handleSetName(name: string) {
    conn?.reducers.setName({ name });
    setHasSetName(true);
  }

  function handleCreateRoom(name: string) {
    conn?.reducers.createRoom({ name });
  }

  function handleJoinRoom(roomId: bigint) {
    conn?.reducers.joinRoom({ roomId });
    setActiveRoomId(roomId);
  }

  function handleLeaveRoom(roomId: bigint) {
    conn?.reducers.leaveRoom({ roomId });
    if (activeRoomId === roomId) setActiveRoomId(null);
  }

  function handleSelectRoom(roomId: bigint) {
    setActiveRoomId(roomId);
    setIsScrolledUp(false);
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!activeRoomId || !messageText.trim()) return;
    conn?.reducers.sendMessage({ roomId: activeRoomId, text: messageText.trim() });
    setMessageText('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    sendTypingStop();
    setIsScrolledUp(false);
  }

  // ── loading / setup state ───────────────────────────────────────────────────

  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        <p>Connecting to SpacetimeDB…</p>
      </div>
    );
  }

  if (!myUser && !hasSetName) {
    return <SetupScreen onSetName={handleSetName} />;
  }

  const activeRoom = rooms.find(r => r.id === activeRoomId);

  // ── render ──────────────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1 className="app-title">SpacetimeDB Chat</h1>
          {myUser && (
            <div className="my-user">
              <span className="status-dot online" />
              <span className="user-name" style={{ color: colorForName(myUser.name) }}>
                {myUser.name}
              </span>
            </div>
          )}
        </div>

        {/* Room list */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="btn btn-icon" onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
          </div>

          {joinedRooms.length === 0 && otherRooms.length === 0 && (
            <p className="empty-state">Create a room to get started</p>
          )}

          {joinedRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <div
                key={String(room.id)}
                className={`room-item ${activeRoomId === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room.id)}
              >
                <span className="room-prefix">#</span>
                <span className="room-name">{room.name}</span>
                {unread > 0 && <span className="badge">{unread}</span>}
              </div>
            );
          })}

          {otherRooms.length > 0 && (
            <>
              <div className="sidebar-subsection">Other rooms</div>
              {otherRooms.map(room => (
                <div key={String(room.id)} className="room-item room-item-other">
                  <span className="room-prefix">#</span>
                  <span className="room-name">{room.name}</span>
                  <button
                    className="btn btn-tiny"
                    onClick={e => { e.stopPropagation(); handleJoinRoom(room.id); }}
                  >
                    Join
                  </button>
                </div>
              ))}
            </>
          )}
        </div>

        {/* Online users */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Online — {onlineUsers.length}</span>
          </div>
          {onlineUsers.map(user => (
            <div key={idHex(user.identity)} className="user-item">
              <span className="status-dot online" />
              <span className="user-name" style={{ color: colorForName(user.name) }}>
                {user.name}
                {myIdentity && idHex(user.identity) === idHex(myIdentity) ? ' (you)' : ''}
              </span>
            </div>
          ))}
        </div>
      </aside>

      {/* Main area */}
      <main className="main">
        {!activeRoom ? (
          <div className="empty-main">
            <p className="empty-state">Select a room to start chatting</p>
          </div>
        ) : (
          <>
            {/* Room header */}
            <header className="room-header">
              <div className="room-header-left">
                <span className="room-prefix">#</span>
                <h2 className="room-header-name">{activeRoom.name}</h2>
              </div>
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => handleLeaveRoom(activeRoom.id)}
              >
                Leave
              </button>
            </header>

            {/* Messages */}
            <div
              className="messages"
              ref={messagesContainerRef}
              onScroll={handleScroll}
            >
              {activeRoomMessages.length === 0 && (
                <p className="empty-state center">No messages yet. Say hello!</p>
              )}

              {activeRoomMessages.map((msg, idx) => {
                const sender = users.find(u => idHex(u.identity) === idHex(msg.senderIdentity));
                const isMe = myIdentity && idHex(msg.senderIdentity) === idHex(myIdentity);
                const prevMsg = idx > 0 ? activeRoomMessages[idx - 1] : null;
                const isGrouped = prevMsg && idHex(prevMsg.senderIdentity) === idHex(msg.senderIdentity);
                const readers = getReadersForMessage(msg);

                return (
                  <div key={String(msg.id)} className={`message-group ${isGrouped ? 'grouped' : ''}`}>
                    {!isGrouped && (
                      <div className="message-header">
                        <span
                          className="message-sender"
                          style={{ color: colorForName(sender?.name ?? '?') }}
                        >
                          {sender?.name ?? 'Unknown'}
                          {isMe ? ' (you)' : ''}
                        </span>
                        <span className="message-time">{formatTime(msg.sentAt)}</span>
                      </div>
                    )}
                    <div className="message-text">{msg.text}</div>
                    {readers.length > 0 && (
                      <div className="read-receipt">
                        Seen by {readers.join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}

              {/* Typing indicator */}
              {activeTypers.length > 0 && (
                <div className="typing-indicator">
                  {activeTypers.length === 1
                    ? (() => {
                        const u = users.find(u => idHex(u.identity) === idHex(activeTypers[0].userIdentity));
                        return `${u?.name ?? 'Someone'} is typing…`;
                      })()
                    : 'Multiple users are typing…'}
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>

            {/* Scroll to bottom button */}
            {isScrolledUp && (
              <button
                className="scroll-to-bottom"
                onClick={() => {
                  setIsScrolledUp(false);
                  messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
                }}
              >
                ↓ New messages
              </button>
            )}

            {/* Message input */}
            <form className="message-input-bar" onSubmit={handleSendMessage}>
              <input
                className="input message-input"
                type="text"
                placeholder="Type a message…"
                value={messageText}
                onChange={handleMessageInput}
              />
              <button type="submit" className="btn btn-primary" disabled={!messageText.trim()}>
                Send
              </button>
            </form>
          </>
        )}
      </main>

      {showCreateRoom && (
        <CreateRoomModal
          onClose={() => setShowCreateRoom(false)}
          onCreate={handleCreateRoom}
        />
      )}
    </div>
  );
}
