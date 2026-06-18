import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, Room, User, UserRoomRead, TypingIndicator } from './module_bindings/types';

// ---- helpers ----

function tsToMs(ts: { microsSinceUnixEpoch: bigint }): number {
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  return new Date(tsToMs(ts)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

const NAME_COLORS = ['#4cf490', '#a880ff', '#02befa', '#fbdc8e', '#ff4c4c', '#4cf4d8', '#f490c4'];
function nameColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) & 0xffffff;
  return NAME_COLORS[Math.abs(hash) % NAME_COLORS.length];
}

const TYPING_TIMEOUT_MS = 5000;
const TYPING_DEBOUNCE_MS = 3000;

// ---- MessageList ----

interface MessageListProps {
  messages: readonly Message[];
  users: readonly User[];
  myIdentity: { toHexString(): string } | null | undefined;
  userRoomReads: readonly UserRoomRead[];
}

function MessageList({ messages, users, myIdentity, userRoomReads }: MessageListProps) {
  const getUserByIdentity = (hex: string): User | undefined =>
    users.find(u => u.identity.toHexString() === hex);

  // For each message, find users whose lastReadMessageId equals this message's id
  // (i.e., this is the latest message they've read)
  const getExactReaders = (msg: Message, idx: number): User[] => {
    const nextMsg = messages[idx + 1];
    return userRoomReads
      .filter(r => {
        if (r.roomId !== msg.roomId) return false;
        if (r.lastReadMessageId < msg.id) return false;
        if (nextMsg && r.lastReadMessageId >= nextMsg.id) return false;
        return true;
      })
      .map(r => getUserByIdentity(r.userIdentity.toHexString()))
      .filter((u): u is User => u !== undefined)
      .filter(u => u.identity.toHexString() !== msg.sender.toHexString());
  };

  // Group consecutive messages from same sender
  type Group = { sender: User | undefined; senderHex: string; msgs: { msg: Message; idx: number }[] };
  const groups: Group[] = [];
  messages.forEach((msg, idx) => {
    const senderHex = msg.sender.toHexString();
    const last = groups[groups.length - 1];
    if (last && last.senderHex === senderHex) {
      last.msgs.push({ msg, idx });
    } else {
      groups.push({
        sender: getUserByIdentity(senderHex),
        senderHex,
        msgs: [{ msg, idx }],
      });
    }
  });

  return (
    <div className="message-list">
      {groups.map(group => (
        <div key={String(group.msgs[0].msg.id)} className="message-group">
          <div className="message-group-header">
            <div
              className="sender-avatar"
              style={{ background: nameColor(group.sender?.name ?? '?') }}
            >
              {(group.sender?.name?.[0] ?? '?').toUpperCase()}
            </div>
            <span className="sender-name" style={{ color: nameColor(group.sender?.name ?? '?') }}>
              {group.sender?.name ?? 'Unknown'}
              {group.sender?.identity.toHexString() === myIdentity?.toHexString() && (
                <span className="you-label"> (you)</span>
              )}
            </span>
            <span className="message-time">{formatTime(group.msgs[0].msg.sentAt)}</span>
          </div>
          {group.msgs.map(({ msg, idx }) => {
            const readers = getExactReaders(msg, idx);
            return (
              <div key={String(msg.id)} className="message-row">
                <div className="message-content">{msg.content}</div>
                {readers.length > 0 && (
                  <div className="read-receipt">
                    Seen by {readers.map(u => u.name).join(', ')}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ---- App ----

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [nameError, setNameError] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showNewRoom, setShowNewRoom] = useState(false);
  const [roomError, setRoomError] = useState('');
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [typingActive, setTypingActive] = useState(false);
  const [, setTick] = useState(0);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const selectedRoomIdRef = useRef<bigint | null>(null);
  selectedRoomIdRef.current = selectedRoomId;

  // Refresh every second for typing indicator age
  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.roomMember,
        tables.message,
        tables.typingIndicator,
        tables.userRoomRead,
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [userRoomReads] = useTable(tables.userRoomRead);

  const myHex = myIdentity?.toHexString();
  const myUser = users.find(u => u.identity.toHexString() === myHex);
  const myMemberships = roomMembers.filter(m => m.userIdentity.toHexString() === myHex);
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));
  const myRooms = rooms
    .filter(r => myRoomIds.has(r.id))
    .sort((a, b) => {
      const d = a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });
  const otherRooms = rooms.filter(r => !myRoomIds.has(r.id));
  const onlineUsers = users.filter(u => u.online && u.name !== '');

  const selectedRoom = rooms.find(r => r.id === selectedRoomId);
  const roomMessages = messages
    .filter(m => m.roomId === selectedRoomId)
    .sort((a, b) => {
      const d = a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  // Unread count
  const getUnreadCount = (roomId: bigint): number => {
    const read = userRoomReads.find(
      r => r.roomId === roomId && r.userIdentity.toHexString() === myHex
    );
    const lastReadId = read?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastReadId).length;
  };

  // Typing users in selected room (excluding self, not expired)
  const typingUsers = selectedRoomId
    ? typingIndicators
        .filter(ti => {
          if (ti.roomId !== selectedRoomId) return false;
          if (ti.userIdentity.toHexString() === myHex) return false;
          return Date.now() - tsToMs(ti.updatedAt) < TYPING_TIMEOUT_MS;
        })
        .map(ti => users.find(u => u.identity.toHexString() === ti.userIdentity.toHexString()))
        .filter((u): u is User => u !== undefined)
    : [];

  // Mark as read when messages arrive in selected room
  useEffect(() => {
    if (!conn || !selectedRoomId || roomMessages.length === 0) return;
    const latest = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: latest.id });
  }, [selectedRoomId, roomMessages.length]);

  // Auto-scroll
  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  useEffect(() => {
    if (isAtBottom) scrollToBottom();
  }, [roomMessages.length, isAtBottom, scrollToBottom]);

  useEffect(() => {
    setIsAtBottom(true);
    setTimeout(() => messagesEndRef.current?.scrollIntoView(), 50);
    setMessageInput('');
  }, [selectedRoomId]);

  const handleScroll = () => {
    const c = messagesContainerRef.current;
    if (!c) return;
    setIsAtBottom(c.scrollHeight - c.scrollTop - c.clientHeight < 60);
  };

  // Typing
  const stopTyping = useCallback(() => {
    if (!conn || !selectedRoomIdRef.current) return;
    conn.reducers.updateTyping({ roomId: selectedRoomIdRef.current, isTyping: false });
    setTypingActive(false);
  }, [conn]);

  const handleTyping = (value: string) => {
    setMessageInput(value);
    if (!conn || !selectedRoomId) return;

    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);

    if (value.length > 0) {
      if (!typingActive) {
        conn.reducers.updateTyping({ roomId: selectedRoomId, isTyping: true });
        setTypingActive(true);
      }
      typingTimerRef.current = setTimeout(stopTyping, TYPING_DEBOUNCE_MS);
    } else {
      stopTyping();
    }
  };

  // Clear typing on room change
  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      stopTyping();
    };
  }, [selectedRoomId, stopTyping]);

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({ roomId: selectedRoomId, content: messageInput.trim() });
    setMessageInput('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    stopTyping();
    setIsAtBottom(true);
  };

  const handleSetName = () => {
    if (!conn || !nameInput.trim()) { setNameError('Please enter a display name'); return; }
    conn.reducers.setName({ name: nameInput.trim() });
    setNameError('');
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) { setRoomError('Please enter a room name'); return; }
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowNewRoom(false);
    setRoomError('');
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
    setSelectedRoomId(roomId);
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  };

  // ---- Connecting screen ----
  if (!isActive || !subscribed) {
    return (
      <div className="fullscreen-center">
        <div className="connect-card">
          <div className="spinner" />
          <h2 className="gradient-title">SpacetimeDB Chat</h2>
          <p className="muted">Connecting to server...</p>
        </div>
      </div>
    );
  }

  // ---- Name setup screen ----
  if (!myUser || myUser.name === '') {
    return (
      <div className="fullscreen-center">
        <div className="connect-card">
          <h1 className="gradient-title">SpacetimeDB Chat</h1>
          <p className="muted">Choose a display name to get started</p>
          <div className="name-input-row">
            <input
              type="text"
              placeholder="Your display name..."
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSetName()}
              maxLength={32}
              autoFocus
            />
            <button className="btn-primary" onClick={handleSetName}>
              Join
            </button>
          </div>
          {nameError && <p className="error-msg">{nameError}</p>}
        </div>
      </div>
    );
  }

  // ---- Main UI ----
  return (
    <div className="app-layout">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-brand">
          <span className="gradient-title brand-title">SpacetimeDB Chat</span>
        </div>

        <div className="sidebar-me">
          <div className="avatar" style={{ background: nameColor(myUser.name) }}>
            {myUser.name[0].toUpperCase()}
          </div>
          <div className="sidebar-me-info">
            <span className="sidebar-me-name">{myUser.name}</span>
            <span className="status-row">
              <span className="dot dot-online" />
              <span className="muted small">Online</span>
            </span>
          </div>
        </div>

        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowNewRoom(true)} title="Create room">
              +
            </button>
          </div>

          {myRooms.length === 0 && (
            <p className="empty-hint">No rooms yet — create one!</p>
          )}

          {myRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <button
                key={String(room.id)}
                className={`room-btn ${selectedRoomId === room.id ? 'active' : ''}`}
                onClick={() => setSelectedRoomId(room.id)}
              >
                <span className="room-hash">#</span>
                <span className="room-btn-name">{room.name}</span>
                {unread > 0 && <span className="badge">{unread}</span>}
              </button>
            );
          })}

          {otherRooms.length > 0 && (
            <>
              <div className="subsection-label">Other Rooms</div>
              {otherRooms.map(room => (
                <div key={String(room.id)} className="room-btn other-room">
                  <span className="room-hash">#</span>
                  <span className="room-btn-name">{room.name}</span>
                  <button className="join-btn" onClick={() => handleJoinRoom(room.id)}>
                    Join
                  </button>
                </div>
              ))}
            </>
          )}
        </div>

        <div className="sidebar-section online-section">
          <div className="section-header">
            <span>Online — {onlineUsers.length}</span>
          </div>
          {onlineUsers.map(u => (
            <div key={u.identity.toHexString()} className="online-user">
              <span className="dot dot-online" />
              <span className={u.identity.toHexString() === myHex ? 'font-bold' : ''}>
                {u.name}
                {u.identity.toHexString() === myHex && <span className="muted small"> (you)</span>}
              </span>
            </div>
          ))}
        </div>
      </aside>

      {/* Main */}
      <main className="chat-main">
        {!selectedRoom ? (
          <div className="fullscreen-center flex-1">
            <div className="welcome-card">
              <h2>Welcome, {myUser.name}!</h2>
              <p className="muted">Select a room from the sidebar or create a new one.</p>
              <button className="btn-primary" onClick={() => setShowNewRoom(true)}>
                Create a Room
              </button>
            </div>
          </div>
        ) : (
          <div className="chat-layout">
            {/* Header */}
            <div className="chat-header">
              <div className="chat-header-left">
                <span className="room-hash-lg">#</span>
                <h2 className="chat-room-title">{selectedRoom.name}</h2>
              </div>
              <button
                className="btn-ghost"
                onClick={() => handleLeaveRoom(selectedRoom.id)}
              >
                Leave
              </button>
            </div>

            {/* Messages */}
            <div
              ref={messagesContainerRef}
              className="messages-area"
              onScroll={handleScroll}
            >
              {roomMessages.length === 0 ? (
                <div className="fullscreen-center flex-1">
                  <p className="muted">No messages yet — say something!</p>
                </div>
              ) : (
                <MessageList
                  messages={roomMessages}
                  users={users}
                  myIdentity={myIdentity}
                  userRoomReads={userRoomReads}
                />
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Scroll to bottom */}
            {!isAtBottom && (
              <button
                className="scroll-btn"
                onClick={() => { scrollToBottom(); setIsAtBottom(true); }}
              >
                ↓ Scroll to latest
              </button>
            )}

            {/* Typing indicator */}
            <div className="typing-row">
              {typingUsers.length > 0 && (
                <span className="typing-text">
                  {typingUsers.length === 1
                    ? `${typingUsers[0].name} is typing...`
                    : `${typingUsers.map(u => u.name).join(', ')} are typing...`}
                </span>
              )}
            </div>

            {/* Input bar */}
            <div className="input-bar">
              <input
                type="text"
                className="message-input"
                placeholder={`Message #${selectedRoom.name}`}
                value={messageInput}
                onChange={e => handleTyping(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    handleSendMessage();
                  }
                }}
                maxLength={2000}
              />
              <button
                className="btn-primary"
                onClick={handleSendMessage}
                disabled={!messageInput.trim()}
              >
                Send
              </button>
            </div>
          </div>
        )}
      </main>

      {/* New Room Modal */}
      {showNewRoom && (
        <div
          className="modal-backdrop"
          onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Create a Room</h3>
            <input
              type="text"
              placeholder="Room name..."
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') handleCreateRoom();
                if (e.key === 'Escape') { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }
              }}
              maxLength={64}
              autoFocus
            />
            {roomError && <p className="error-msg">{roomError}</p>}
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }}
              >
                Cancel
              </button>
              <button className="btn-primary" onClick={handleCreateRoom}>
                Create
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
