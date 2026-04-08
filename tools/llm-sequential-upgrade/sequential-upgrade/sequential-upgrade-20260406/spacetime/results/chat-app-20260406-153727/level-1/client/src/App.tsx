import { useEffect, useRef, useState, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

type DbConn = DbConnection | null;

function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function formatTime(d: Date): string {
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConn;

  const [subscribed, setSubscribed] = useState(false);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [showNameModal, setShowNameModal] = useState(false);
  const [showCreateRoomModal, setShowCreateRoomModal] = useState(false);
  const [nameInput, setNameInput] = useState('');
  const [roomInput, setRoomInput] = useState('');
  const [messageInput, setMessageInput] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [showScrollBtn, setShowScrollBtn] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isAtBottomRef = useRef(true);

  // Save token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe
  useEffect(() => {
    if (!conn || !isActive) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
      ]);
  }, [conn, isActive]);

  // Check if user has a name
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);

  const myUser = myIdentity
    ? users.find((u) => u.identity.toHexString() === myIdentity.toHexString())
    : null;

  // Show name modal if connected but no name set
  useEffect(() => {
    if (isActive && subscribed && !myUser) {
      setShowNameModal(true);
    }
  }, [isActive, subscribed, myUser]);

  // Membership helpers
  const isMember = useCallback(
    (roomId: bigint) => {
      if (!myIdentity) return false;
      return roomMembers.some(
        (m) =>
          m.roomId === roomId &&
          m.userIdentity.toHexString() === myIdentity.toHexString()
      );
    },
    [roomMembers, myIdentity]
  );

  // Messages for selected room, sorted by sentAt
  const roomMessages = messages
    .filter((m) => m.roomId === selectedRoomId)
    .sort((a, b) =>
      Number(a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch)
    );

  // Auto scroll
  useEffect(() => {
    if (isAtBottomRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [roomMessages.length]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!conn || !isActive || !selectedRoomId || !myIdentity) return;
    const roomMsgs = messages.filter((m) => m.roomId === selectedRoomId);
    if (roomMsgs.length === 0) return;
    const maxId = roomMsgs.reduce(
      (max, m) => (m.id > max ? m.id : max),
      0n
    );
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: maxId });
  }, [conn, isActive, selectedRoomId, messages, myIdentity]);

  const handleScroll = () => {
    const el = messagesContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    isAtBottomRef.current = atBottom;
    setShowScrollBtn(!atBottom);
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  const showError = (msg: string) => {
    setError(msg);
    setTimeout(() => setError(null), 4000);
  };

  const handleSetName = () => {
    if (!conn || !nameInput.trim()) return;
    try {
      conn.reducers.setName({ name: nameInput.trim() });
      setShowNameModal(false);
      setNameInput('');
    } catch (e) {
      showError(String(e));
    }
  };

  const handleCreateRoom = () => {
    if (!conn || !roomInput.trim()) return;
    try {
      conn.reducers.createRoom({ name: roomInput.trim() });
      setShowCreateRoomModal(false);
      setRoomInput('');
    } catch (e) {
      showError(String(e));
    }
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    try {
      conn.reducers.joinRoom({ roomId });
    } catch (e) {
      showError(String(e));
    }
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    try {
      conn.reducers.leaveRoom({ roomId });
      if (selectedRoomId === roomId) setSelectedRoomId(null);
    } catch (e) {
      showError(String(e));
    }
  };

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    try {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageInput.trim() });
      setMessageInput('');
      // Clear typing
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
    } catch (e) {
      showError(String(e));
    }
  };

  const handleTyping = (value: string) => {
    setMessageInput(value);
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: true });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.setTyping({ roomId: selectedRoomId!, isTyping: false });
    }, 3000);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
    if (e.key === 'Escape') {
      setMessageInput('');
      if (conn && selectedRoomId) {
        conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
      }
    }
  };

  // Typing users in selected room (excluding me)
  const typingUsers = selectedRoomId
    ? typingIndicators
        .filter(
          (ti) =>
            ti.roomId === selectedRoomId &&
            myIdentity &&
            ti.userIdentity.toHexString() !== myIdentity.toHexString()
        )
        .map((ti) => users.find((u) => u.identity.toHexString() === ti.userIdentity.toHexString())?.name ?? 'Someone')
    : [];

  // Unread count per room
  const unreadCount = (roomId: bigint): number => {
    if (!myIdentity) return 0;
    const receipt = readReceipts.find(
      (r) =>
        r.roomId === roomId &&
        r.userIdentity.toHexString() === myIdentity.toHexString()
    );
    const lastReadId = receipt?.lastReadMessageId ?? 0n;
    return messages.filter(
      (m) =>
        m.roomId === roomId &&
        m.id > lastReadId &&
        m.senderIdentity.toHexString() !== myIdentity.toHexString()
    ).length;
  };

  // Read receipt display for a message
  const seenBy = (msgId: bigint): string[] => {
    if (!myIdentity) return [];
    return readReceipts
      .filter(
        (r) =>
          r.lastReadMessageId >= msgId &&
          r.userIdentity.toHexString() !== myIdentity.toHexString()
      )
      .map((r) => users.find((u) => u.identity.toHexString() === r.userIdentity.toHexString())?.name ?? '?');
  };

  // Online users
  const onlineUsers = users.filter((u) => u.online);

  if (!isActive || !subscribed) {
    return (
      <div className="app">
        <div className="loading-screen">
          <div className="spinner" />
          <span>Connecting to SpacetimeDB...</span>
        </div>
      </div>
    );
  }

  const selectedRoom = selectedRoomId ? rooms.find((r) => r.id === selectedRoomId) : null;
  const inSelectedRoom = selectedRoomId ? isMember(selectedRoomId) : false;

  // Sort rooms by name
  const sortedRooms = [...rooms].sort((a, b) => a.name.localeCompare(b.name));

  // Members of selected room
  const selectedRoomMembers = selectedRoomId
    ? roomMembers
        .filter((m) => m.roomId === selectedRoomId)
        .map((m) => users.find((u) => u.identity.toHexString() === m.userIdentity.toHexString()))
        .filter(Boolean)
    : [];

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <div className="sidebar-title">SpacetimeDB Chat</div>
        </div>

        <div className="sidebar-section" style={{ borderBottom: '1px solid var(--border)', paddingBottom: 8 }}>
          <div className="sidebar-section-label" style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            Rooms
            <button className="create-room-btn" onClick={() => setShowCreateRoomModal(true)} title="Create room">
              +
            </button>
          </div>
          {sortedRooms.length === 0 && (
            <div style={{ padding: '8px 16px', fontSize: 13, color: 'var(--text-muted)' }}>
              No rooms yet
            </div>
          )}
          {sortedRooms.map((room) => {
            const count = unreadCount(room.id);
            return (
              <div
                key={String(room.id)}
                className={`room-item${selectedRoomId === room.id ? ' active' : ''}`}
                onClick={() => setSelectedRoomId(room.id)}
              >
                <span className="room-name">
                  <span className="room-hash">#</span>
                  {room.name}
                </span>
                {count > 0 && <span className="unread-badge">{count}</span>}
              </div>
            );
          })}
        </div>

        <div className="sidebar-section">
          <div className="sidebar-section-label">Online — {onlineUsers.length}</div>
          {onlineUsers.map((u) => (
            <div key={u.identity.toHexString()} className="user-item">
              <span className="status-dot online" />
              {u.name}
              {myIdentity && u.identity.toHexString() === myIdentity.toHexString() && (
                <span style={{ color: 'var(--text-muted)', fontSize: 11 }}> (you)</span>
              )}
            </div>
          ))}
        </div>

        <div className="sidebar-user">
          <span className="status-dot online" />
          <span className="sidebar-user-name">{myUser?.name ?? '...'}</span>
          <button className="sidebar-user-edit" onClick={() => { setNameInput(myUser?.name ?? ''); setShowNameModal(true); }}>
            Edit
          </button>
        </div>
      </div>

      {/* Main area */}
      <div className="main">
        {!selectedRoom ? (
          <div className="no-room-selected">Select a room to start chatting</div>
        ) : (
          <>
            {/* Room header */}
            <div className="room-header">
              <span className="room-header-hash">#</span>
              <span className="room-header-name">{selectedRoom.name}</span>
              <span style={{ fontSize: 12, color: 'var(--text-muted)', marginLeft: 8 }}>
                {selectedRoomMembers.length} members
              </span>
              {inSelectedRoom ? (
                <button className="join-leave-btn leave" onClick={() => handleLeaveRoom(selectedRoom.id)}>
                  Leave
                </button>
              ) : (
                <button className="join-leave-btn" onClick={() => handleJoinRoom(selectedRoom.id)}>
                  Join
                </button>
              )}
            </div>

            {/* Messages */}
            <div
              className="messages-area"
              ref={messagesContainerRef}
              onScroll={handleScroll}
              style={{ position: 'relative' }}
            >
              {roomMessages.length === 0 ? (
                <div className="messages-empty">No messages yet. Say hello!</div>
              ) : (
                roomMessages.map((msg, idx) => {
                  const sender = users.find(
                    (u) => u.identity.toHexString() === msg.senderIdentity.toHexString()
                  );
                  const isMe = myIdentity && msg.senderIdentity.toHexString() === myIdentity.toHexString();
                  const prevMsg = idx > 0 ? roomMessages[idx - 1] : null;
                  const sameSenderAsPrev =
                    prevMsg &&
                    prevMsg.senderIdentity.toHexString() === msg.senderIdentity.toHexString();
                  const seen = seenBy(msg.id);
                  // Only show seen on last message in group from same sender
                  const nextMsg = idx < roomMessages.length - 1 ? roomMessages[idx + 1] : null;
                  const isLastInGroup =
                    !nextMsg ||
                    nextMsg.senderIdentity.toHexString() !== msg.senderIdentity.toHexString();

                  return (
                    <div key={String(msg.id)} className="message-group" style={{ paddingTop: sameSenderAsPrev ? 1 : 10 }}>
                      {!sameSenderAsPrev && (
                        <div className="message-header">
                          <span className={`message-sender${isMe ? ' is-me' : ''}`}>
                            {sender?.name ?? 'Unknown'}
                          </span>
                          <span className="message-time">{formatTime(tsToDate(msg.sentAt))}</span>
                        </div>
                      )}
                      <div className="message-text">{msg.text}</div>
                      {isLastInGroup && seen.length > 0 && (
                        <div className="message-read-by">Seen by {seen.join(', ')}</div>
                      )}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {showScrollBtn && (
              <button className="scroll-bottom-btn" onClick={scrollToBottom}>
                ↓ New messages
              </button>
            )}

            {/* Typing indicator */}
            <div className="typing-indicator">
              {typingUsers.length === 1 && `${typingUsers[0]} is typing...`}
              {typingUsers.length === 2 && `${typingUsers[0]} and ${typingUsers[1]} are typing...`}
              {typingUsers.length > 2 && 'Multiple users are typing...'}
            </div>

            {/* Input */}
            {inSelectedRoom ? (
              <div className="input-bar">
                <textarea
                  className="message-input"
                  placeholder={`Message #${selectedRoom.name}`}
                  value={messageInput}
                  onChange={(e) => handleTyping(e.target.value)}
                  onKeyDown={handleKeyDown}
                  rows={1}
                />
                <button
                  className="send-btn"
                  onClick={handleSendMessage}
                  disabled={!messageInput.trim()}
                >
                  Send
                </button>
              </div>
            ) : (
              <div className="not-member-notice">
                <span>You're not a member of this room.</span>
                <button className="btn-primary" onClick={() => handleJoinRoom(selectedRoom.id)}>
                  Join Room
                </button>
              </div>
            )}
          </>
        )}
      </div>

      {/* Set name modal */}
      {showNameModal && (
        <div className="modal-backdrop" onClick={(e) => { if (e.target === e.currentTarget && myUser) setShowNameModal(false); }}>
          <div className="modal">
            <div className="modal-title">{myUser ? 'Edit your name' : 'Welcome! Set your name'}</div>
            <input
              className="modal-input"
              placeholder="Your display name"
              value={nameInput}
              onChange={(e) => setNameInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') handleSetName(); if (e.key === 'Escape' && myUser) setShowNameModal(false); }}
              autoFocus
              maxLength={32}
            />
            <div className="modal-actions">
              {myUser && (
                <button className="btn-secondary" onClick={() => setShowNameModal(false)}>Cancel</button>
              )}
              <button className="btn-primary" onClick={handleSetName} disabled={!nameInput.trim()}>
                {myUser ? 'Save' : 'Start chatting'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Create room modal */}
      {showCreateRoomModal && (
        <div className="modal-backdrop" onClick={(e) => { if (e.target === e.currentTarget) setShowCreateRoomModal(false); }}>
          <div className="modal">
            <div className="modal-title">Create a room</div>
            <input
              className="modal-input"
              placeholder="Room name"
              value={roomInput}
              onChange={(e) => setRoomInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter') handleCreateRoom(); if (e.key === 'Escape') setShowCreateRoomModal(false); }}
              autoFocus
              maxLength={64}
            />
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => setShowCreateRoomModal(false)}>Cancel</button>
              <button className="btn-primary" onClick={handleCreateRoom} disabled={!roomInput.trim()}>
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Error toast */}
      {error && <div className="error-toast">{error}</div>}
    </div>
  );
}
