import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable } from 'spacetimedb/react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';

// ── Types ──────────────────────────────────────────────────────────────────

type User = { identity: Identity; name: string; online: boolean; createdAt: { microsSinceUnixEpoch: bigint } };
type Room = { id: bigint; name: string; createdBy: Identity; createdAt: { microsSinceUnixEpoch: bigint } };
type RoomMember = { id: bigint; roomId: bigint; memberIdentity: Identity; joinedAt: { microsSinceUnixEpoch: bigint } };
type Message = { id: bigint; roomId: bigint; senderId: Identity; text: string; sentAt: { microsSinceUnixEpoch: bigint } };
type TypingIndicator = { id: bigint; roomId: bigint; userIdentity: Identity; expiresAt: { microsSinceUnixEpoch: bigint } };
type ReadReceipt = { id: bigint; messageId: bigint; userIdentity: Identity; seenAt: { microsSinceUnixEpoch: bigint } };
type UserRoomRead = { id: bigint; userIdentity: Identity; roomId: bigint; lastReadMessageId: bigint; lastReadAt: { microsSinceUnixEpoch: bigint } };

function idHex(id: Identity) {
  return id.toHexString();
}

function tsMs(ts: { microsSinceUnixEpoch: bigint }) {
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function tsDate(ts: { microsSinceUnixEpoch: bigint }) {
  return new Date(tsMs(ts));
}

// ── App ────────────────────────────────────────────────────────────────────

export default function App() {
  const [conn, setConn] = useState<DbConnection | null>(window.__db_conn);
  const [myIdentity, setMyIdentity] = useState<Identity | null>(window.__my_identity);

  const [displayName, setDisplayName] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [, forceUpdate] = useState(0);

  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Poll for connection establishment
  useEffect(() => {
    if (window.__db_conn && !conn) setConn(window.__db_conn);
    if (window.__my_identity && !myIdentity) setMyIdentity(window.__my_identity);
    const interval = setInterval(() => {
      if (window.__db_conn && !conn) setConn(window.__db_conn);
      if (window.__my_identity && !myIdentity) setMyIdentity(window.__my_identity);
    }, 100);
    return () => clearInterval(interval);
  }, [conn, myIdentity]);

  // Tick every second for typing indicator expiry display
  useEffect(() => {
    const interval = setInterval(() => forceUpdate(n => n + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // ── Data from SpacetimeDB ────────────────────────────────────────────────
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [usersRaw] = useTable(tables['user'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [roomsRaw] = useTable(tables['room'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [roomMembersRaw] = useTable(tables['room_member'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [messagesRaw] = useTable(tables['message'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [typingRaw] = useTable(tables['typing_indicator'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [readReceiptsRaw] = useTable(tables['read_receipt'] as any);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [userRoomReadsRaw] = useTable(tables['user_room_read'] as any);

  const users = (usersRaw || []) as User[];
  const rooms = (roomsRaw || []) as Room[];
  const roomMembers = (roomMembersRaw || []) as RoomMember[];
  const messages = (messagesRaw || []) as Message[];
  const typingIndicators = (typingRaw || []) as TypingIndicator[];
  const readReceipts = (readReceiptsRaw || []) as ReadReceipt[];
  const userRoomReads = (userRoomReadsRaw || []) as UserRoomRead[];

  const currentUser = myIdentity ? users.find(u => idHex(u.identity) === idHex(myIdentity)) : null;
  const hasName = !!currentUser?.name;

  // My joined rooms
  const myMemberships = myIdentity ? roomMembers.filter(m => idHex(m.memberIdentity) === idHex(myIdentity)) : [];
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));

  // Selected room messages sorted by time
  const roomMessages = messages
    .filter(m => m.roomId === selectedRoomId)
    .sort((a, b) => Number(a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch));

  // ── Typing indicators for current room ────────────────────────────────
  const nowMicros = BigInt(Date.now()) * 1000n;
  const activeTypers = selectedRoomId && myIdentity
    ? typingIndicators.filter(ti =>
        ti.roomId === selectedRoomId &&
        idHex(ti.userIdentity) !== idHex(myIdentity) &&
        ti.expiresAt.microsSinceUnixEpoch > nowMicros
      )
    : [];

  const typerNames = activeTypers.map(ti => {
    const u = users.find(u => idHex(u.identity) === idHex(ti.userIdentity));
    return u?.name || 'Someone';
  });

  // ── Unread counts ──────────────────────────────────────────────────────
  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const lastRead = userRoomReads.find(r =>
      idHex(r.userIdentity) === idHex(myIdentity) && r.roomId === roomId
    );
    const lastReadId = lastRead?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastReadId).length;
  }

  // ── Auto-scroll to bottom ─────────────────────────────────────────────
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [roomMessages.length]);

  // ── Mark messages as read when viewing ────────────────────────────────
  useEffect(() => {
    if (!conn || !selectedRoomId || roomMessages.length === 0) return;
    const lastMsg = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [selectedRoomId, roomMessages.length, conn]);

  // ── Typing indicator logic ─────────────────────────────────────────────
  const handleTyping = useCallback(() => {
    if (!conn || !selectedRoomId) return;
    if (!isTypingRef.current) {
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: true });
      isTypingRef.current = true;
    }
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      if (conn && selectedRoomId) {
        conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
      }
      isTypingRef.current = false;
    }, 3000);
  }, [conn, selectedRoomId]);

  // Clear typing when switching rooms
  useEffect(() => {
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    if (conn && isTypingRef.current) {
      // clear old room typing on switch (best effort)
      isTypingRef.current = false;
    }
  }, [selectedRoomId, conn]);

  // ── Actions ─────────────────────────────────────────────────────────────

  const handleSetName = () => {
    if (!conn || !nameInput.trim()) return;
    conn.reducers.setName({ name: nameInput.trim() });
    setNameInput('');
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowCreateRoom(false);
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = () => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.leaveRoom({ roomId: selectedRoomId });
    setSelectedRoomId(null);
  };

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageInput.trim() });
    setMessageInput('');
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    isTypingRef.current = false;
  };

  const handleMessageKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  };

  // ── Read receipts for a message ──────────────────────────────────────
  function getSeenBy(messageId: bigint): string[] {
    return readReceipts
      .filter(rr => rr.messageId === messageId)
      .map(rr => {
        const u = users.find(u => idHex(u.identity) === idHex(rr.userIdentity));
        return u?.name || 'Unknown';
      })
      .filter(Boolean);
  }

  // ── Render: Setup screen ─────────────────────────────────────────────
  if (!conn || !myIdentity) {
    return (
      <div className="loading">
        <div className="loading-spinner">Connecting to SpacetimeDB...</div>
      </div>
    );
  }

  if (!hasName) {
    return (
      <div className="setup-screen">
        <div className="setup-card">
          <h1 className="logo">⚡ SpacetimeDB Chat</h1>
          <p className="subtitle">Enter your display name to get started</p>
          <div className="name-input-row">
            <input
              className="input"
              type="text"
              placeholder="Your display name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSetName()}
              maxLength={32}
              autoFocus
            />
            <button className="btn btn-primary" onClick={handleSetName} disabled={!nameInput.trim()}>
              Join
            </button>
          </div>
        </div>
      </div>
    );
  }

  const selectedRoom = rooms.find(r => r.id === selectedRoomId);
  const roomMembersList = selectedRoomId
    ? roomMembers.filter(m => m.roomId === selectedRoomId)
    : [];
  const roomMemberUsers = roomMembersList.map(m =>
    users.find(u => idHex(u.identity) === idHex(m.memberIdentity))
  ).filter(Boolean) as User[];

  // ── Render: Main chat ────────────────────────────────────────────────
  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <span className="logo-small">⚡ Chat</span>
          <span className="user-name">{currentUser?.name}</span>
        </div>

        {/* Online users */}
        <div className="sidebar-section">
          <h3 className="section-title">Online ({users.filter(u => u.online).length})</h3>
          <div className="user-list">
            {users.filter(u => u.online).map(u => (
              <div key={idHex(u.identity)} className="user-item">
                <span className="online-dot" />
                <span>{u.name || '(unnamed)'}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Rooms */}
        <div className="sidebar-section">
          <div className="section-header">
            <h3 className="section-title">Rooms</h3>
            <button className="btn-icon" onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
          </div>

          {showCreateRoom && (
            <div className="create-room-form">
              <input
                className="input input-sm"
                type="text"
                placeholder="Room name"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                autoFocus
              />
              <div className="form-actions">
                <button className="btn btn-primary btn-sm" onClick={handleCreateRoom}>Create</button>
                <button className="btn btn-ghost btn-sm" onClick={() => { setShowCreateRoom(false); setNewRoomName(''); }}>Cancel</button>
              </div>
            </div>
          )}

          <div className="room-list">
            {rooms.map(room => {
              const isMember = myRoomIds.has(room.id);
              const unread = isMember ? getUnreadCount(room.id) : 0;
              const isSelected = room.id === selectedRoomId;
              return (
                <div
                  key={String(room.id)}
                  className={`room-item ${isSelected ? 'active' : ''} ${isMember ? '' : 'not-member'}`}
                  onClick={() => {
                    if (isMember) setSelectedRoomId(room.id);
                    else handleJoinRoom(room.id);
                  }}
                >
                  <span className="room-name"># {room.name}</span>
                  {unread > 0 && <span className="unread-badge">{unread}</span>}
                  {!isMember && <span className="join-hint">Join</span>}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Main content */}
      <div className="main">
        {selectedRoom ? (
          <>
            {/* Room header */}
            <div className="room-header">
              <div className="room-header-left">
                <h2># {selectedRoom.name}</h2>
                <span className="member-count">{roomMembersList.length} members</span>
              </div>
              <div className="room-header-right">
                <div className="members-avatars">
                  {roomMemberUsers.slice(0, 5).map(u => (
                    <span
                      key={idHex(u.identity)}
                      className={`avatar ${u.online ? 'online' : 'offline'}`}
                      title={u.name}
                    >
                      {u.name[0]?.toUpperCase() || '?'}
                    </span>
                  ))}
                </div>
                <button className="btn btn-ghost btn-sm" onClick={handleLeaveRoom}>Leave</button>
              </div>
            </div>

            {/* Messages */}
            <div className="messages-container">
              {roomMessages.length === 0 ? (
                <div className="empty-messages">No messages yet. Say hello!</div>
              ) : (
                roomMessages.map((msg, idx) => {
                  const sender = users.find(u => idHex(u.identity) === idHex(msg.senderId));
                  const isMe = myIdentity && idHex(msg.senderId) === idHex(myIdentity);
                  const seenBy = getSeenBy(msg.id).filter(n => !isMe || n !== currentUser?.name);
                  const isLastMessage = idx === roomMessages.length - 1;

                  return (
                    <div key={String(msg.id)} className={`message ${isMe ? 'mine' : ''}`}>
                      <div className="message-header">
                        <span className="message-sender">{sender?.name || 'Unknown'}</span>
                        <span className="message-time">
                          {tsDate(msg.sentAt).toLocaleTimeString()}
                        </span>
                      </div>
                      <div className="message-body">{msg.text}</div>
                      {seenBy.length > 0 && (
                        <div className="read-receipt">
                          Seen by {seenBy.join(', ')}
                        </div>
                      )}
                      {isLastMessage && seenBy.length === 0 && isMe && (
                        <div className="read-receipt read-receipt-sent">Sent</div>
                      )}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            <div className="typing-indicator-bar">
              {typerNames.length === 1 && (
                <span className="typing-text">{typerNames[0]} is typing...</span>
              )}
              {typerNames.length === 2 && (
                <span className="typing-text">{typerNames[0]} and {typerNames[1]} are typing...</span>
              )}
              {typerNames.length > 2 && (
                <span className="typing-text">Multiple people are typing...</span>
              )}
            </div>

            {/* Message input */}
            <div className="input-bar">
              <textarea
                className="message-input"
                placeholder={`Message #${selectedRoom.name}`}
                value={messageInput}
                onChange={e => {
                  setMessageInput(e.target.value);
                  handleTyping();
                }}
                onKeyDown={handleMessageKeyDown}
                rows={1}
              />
              <button
                className="btn btn-primary"
                onClick={handleSendMessage}
                disabled={!messageInput.trim()}
              >
                Send
              </button>
            </div>
          </>
        ) : (
          <div className="no-room-selected">
            <div className="welcome">
              <h2>Welcome, {currentUser?.name}!</h2>
              <p>Select a room from the sidebar or create a new one to start chatting.</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
