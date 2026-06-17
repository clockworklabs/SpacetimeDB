import { useState, useEffect, useRef } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message } from './module_bindings/types';

const TYPING_EXPIRY_US = 3_000_000n;

function formatTime(microsSinceUnixEpoch: bigint): string {
  const ms = Number(microsSinceUnixEpoch / 1000n);
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function identityHex(identity: { toHexString: () => string }): string {
  return identity.toHexString();
}

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);

  const [subscribed, setSubscribed] = useState(false);
  const [currentRoomId, setCurrentRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const lastTypingRef = useRef<number>(0);
  const subscribedRef = useRef(false);

  // Force re-render every second for typing indicator expiry
  const [, forceRefresh] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => forceRefresh(n => n + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive || subscribedRef.current) return;
    subscribedRef.current = true;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.roomMember,
        tables.message,
        tables.typingIndicator,
        tables.readReceipt,
      ]);
  }, [conn, isActive]);

  const myHex = myIdentity ? identityHex(myIdentity) : null;

  const myUser = users.find(u => identityHex(u.identity) === myHex);
  const hasName = !!(myUser?.name);

  const myMemberRoomIds = new Set(
    roomMembers
      .filter(m => identityHex(m.userIdentity) === myHex)
      .map(m => m.roomId)
  );

  const myRooms = rooms.filter(r => myMemberRoomIds.has(r.id));
  const otherRooms = rooms.filter(r => !myMemberRoomIds.has(r.id));
  const onlineUsers = users.filter(u => u.online && u.name);

  const currentMessages = messages
    .filter(m => m.roomId === currentRoomId)
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

  const nowUs = BigInt(Date.now()) * 1000n;
  const currentTyping = typingIndicators.filter(
    ti =>
      ti.roomId === currentRoomId &&
      identityHex(ti.userIdentity) !== myHex &&
      nowUs - ti.updatedAt.microsSinceUnixEpoch < TYPING_EXPIRY_US
  );

  function getUnreadCount(roomId: bigint): number {
    const roomMsgs = messages.filter(m => m.roomId === roomId);
    if (roomMsgs.length === 0) return 0;
    const myReceipt = readReceipts.find(
      r => r.roomId === roomId && identityHex(r.userIdentity) === myHex
    );
    if (!myReceipt) return roomMsgs.length;
    return roomMsgs.filter(m => m.id > myReceipt.lastReadMessageId).length;
  }

  function getSeenBy(msg: Message): string[] {
    return readReceipts
      .filter(
        r =>
          r.roomId === msg.roomId &&
          r.lastReadMessageId >= msg.id &&
          identityHex(r.userIdentity) !== identityHex(msg.senderIdentity)
      )
      .map(r => users.find(u => identityHex(u.identity) === identityHex(r.userIdentity))?.name)
      .filter((n): n is string => !!n);
  }

  // Auto-scroll when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'instant' });
  }, [currentMessages.length, currentRoomId]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!currentRoomId || !conn || !isActive || !subscribed || currentMessages.length === 0) return;
    const lastMsg = currentMessages[currentMessages.length - 1];
    conn.reducers.markRead({ roomId: currentRoomId, messageId: lastMsg.id });
  }, [currentRoomId, currentMessages.length, conn, isActive, subscribed]);

  function handleSetName(e: React.FormEvent) {
    e.preventDefault();
    if (!nameInput.trim() || !conn || !isActive) return;
    conn.reducers.setName({ name: nameInput.trim() });
    setNameInput('');
  }

  function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    if (!newRoomName.trim() || !conn || !isActive) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowCreateRoom(false);
  }

  function handleJoinRoom(roomId: bigint) {
    if (!conn || !isActive) return;
    conn.reducers.joinRoom({ roomId });
    setCurrentRoomId(roomId);
  }

  function handleLeaveRoom(e: React.MouseEvent, roomId: bigint) {
    e.stopPropagation();
    if (!conn || !isActive) return;
    conn.reducers.leaveRoom({ roomId });
    if (currentRoomId === roomId) setCurrentRoomId(null);
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!messageInput.trim() || !currentRoomId || !conn || !isActive) return;
    conn.reducers.sendMessage({ roomId: currentRoomId, text: messageInput.trim() });
    setMessageInput('');
  }

  function handleInputChange(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageInput(e.target.value);
    const now = Date.now();
    if (currentRoomId && conn && isActive && now - lastTypingRef.current > 1000) {
      lastTypingRef.current = now;
      conn.reducers.setTyping({ roomId: currentRoomId });
    }
  }

  const currentRoom = rooms.find(r => r.id === currentRoomId);

  // Loading / connecting state
  if (!isActive || !subscribed) {
    return (
      <div className="app">
        <div className="loading">
          <div className="loading-text">Connecting to SpacetimeDB...</div>
        </div>
      </div>
    );
  }

  // Name setup
  if (!hasName) {
    return (
      <div className="app">
        <div className="name-setup">
          <div className="name-setup-card">
            <div className="app-title-large">SpacetimeDB Chat</div>
            <div className="name-setup-subtitle">Choose a display name to get started</div>
            <form onSubmit={handleSetName}>
              <input
                className="name-input"
                type="text"
                placeholder="Your name..."
                value={nameInput}
                onChange={e => setNameInput(e.target.value)}
                maxLength={32}
                autoFocus
              />
              <button className="primary-btn" type="submit" disabled={!nameInput.trim()}>
                Join Chat
              </button>
            </form>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <div className="app-title">SpacetimeDB Chat</div>
        </div>

        <div className="user-info">
          <div className="status-dot" />
          <span className="username">{myUser?.name}</span>
        </div>

        {/* My Rooms */}
        <div className="section rooms-section">
          <div className="section-title">Rooms</div>
          {myRooms.length === 0 && (
            <div className="empty-section-text">No rooms yet</div>
          )}
          {myRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <div
                key={String(room.id)}
                className={`room-item ${currentRoomId === room.id ? 'active' : ''}`}
                onClick={() => setCurrentRoomId(room.id)}
              >
                <span className="room-name"># {room.name}</span>
                <div className="room-item-actions">
                  {unread > 0 && currentRoomId !== room.id && (
                    <span className="unread-badge">{unread}</span>
                  )}
                  <button
                    className="leave-btn"
                    onClick={e => handleLeaveRoom(e, room.id)}
                    title="Leave room"
                  >
                    ×
                  </button>
                </div>
              </div>
            );
          })}
          <button className="add-btn" onClick={() => setShowCreateRoom(true)}>
            + Create Room
          </button>
        </div>

        {/* Other Rooms */}
        {otherRooms.length > 0 && (
          <div className="section">
            <div className="section-title">Browse Rooms</div>
            {otherRooms.map(room => (
              <div key={String(room.id)} className="room-item browse-room">
                <span className="room-name"># {room.name}</span>
                <button
                  className="join-btn"
                  onClick={() => handleJoinRoom(room.id)}
                >
                  Join
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Online Users */}
        <div className="section online-section">
          <div className="section-title">Online — {onlineUsers.length}</div>
          {onlineUsers.map(u => (
            <div key={identityHex(u.identity)} className="user-item">
              <div className={`status-dot ${identityHex(u.identity) === myHex ? '' : ''}`} />
              <span>{u.name}{identityHex(u.identity) === myHex ? ' (you)' : ''}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Main Area */}
      <div className="main">
        {!currentRoom ? (
          <div className="empty-state">
            <div className="empty-state-title">Welcome, {myUser?.name}!</div>
            <div className="empty-state-sub">Select a room or create one to start chatting</div>
          </div>
        ) : (
          <>
            {/* Room Header */}
            <div className="room-header">
              <span className="room-header-name"># {currentRoom.name}</span>
              <span className="room-member-count">
                {roomMembers.filter(m => m.roomId === currentRoom.id).length} members
              </span>
            </div>

            {/* Message List */}
            <div className="message-list">
              {currentMessages.length === 0 && (
                <div className="empty-messages">
                  No messages yet. Be the first to say something!
                </div>
              )}
              {currentMessages.map((msg, i) => {
                const prev = i > 0 ? currentMessages[i - 1] : null;
                const isGrouped =
                  prev &&
                  identityHex(prev.senderIdentity) === identityHex(msg.senderIdentity) &&
                  msg.sentAt.microsSinceUnixEpoch - prev.sentAt.microsSinceUnixEpoch < 60_000_000n;
                const sender = users.find(u => identityHex(u.identity) === identityHex(msg.senderIdentity));
                const senderName = sender?.name ?? 'Unknown';
                const isMe = identityHex(msg.senderIdentity) === myHex;
                const seenBy = getSeenBy(msg);

                return (
                  <div key={String(msg.id)} className={`message-wrapper ${isMe ? 'mine' : ''}`}>
                    {!isGrouped && (
                      <div className="message-header">
                        <span className={`message-sender ${isMe ? 'sender-me' : ''}`}>
                          {senderName}
                        </span>
                        <span className="message-time">
                          {formatTime(msg.sentAt.microsSinceUnixEpoch)}
                        </span>
                      </div>
                    )}
                    <div className={isGrouped ? 'message-grouped-text' : 'message-text'}>
                      {isGrouped && (
                        <span className="grouped-time">
                          {formatTime(msg.sentAt.microsSinceUnixEpoch)}
                        </span>
                      )}
                      {msg.text}
                    </div>
                    {seenBy.length > 0 && (
                      <div className="message-seen">
                        Seen by {seenBy.join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Input Area */}
            <div className="input-area">
              <div className="typing-indicator">
                {currentTyping.length === 1 && (
                  <>
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[0].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' is typing...'}
                  </>
                )}
                {currentTyping.length === 2 && (
                  <>
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[0].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' and '}
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[1].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' are typing...'}
                  </>
                )}
                {currentTyping.length > 2 && 'Multiple users are typing...'}
              </div>
              <form className="message-form" onSubmit={handleSendMessage}>
                <input
                  className="message-input"
                  type="text"
                  placeholder={`Message #${currentRoom.name}`}
                  value={messageInput}
                  onChange={handleInputChange}
                  maxLength={2000}
                  autoFocus
                />
                <button
                  className="send-btn"
                  type="submit"
                  disabled={!messageInput.trim()}
                >
                  Send
                </button>
              </form>
            </div>
          </>
        )}
      </div>

      {/* Create Room Modal */}
      {showCreateRoom && (
        <div className="modal-overlay" onClick={() => setShowCreateRoom(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-title">Create a Room</div>
            <form onSubmit={handleCreateRoom}>
              <input
                className="modal-input"
                type="text"
                placeholder="Room name..."
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                maxLength={32}
                autoFocus
              />
              <div className="modal-actions">
                <button
                  type="button"
                  className="cancel-btn"
                  onClick={() => setShowCreateRoom(false)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="primary-btn modal-submit"
                  disabled={!newRoomName.trim()}
                >
                  Create
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
