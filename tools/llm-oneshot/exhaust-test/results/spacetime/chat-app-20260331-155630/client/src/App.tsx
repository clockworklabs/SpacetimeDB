import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { useTable } from 'spacetimedb/react';
import { useSpacetimeDB } from 'spacetimedb/react';
import { tables, reducers } from './module_bindings';
import type { EventContext } from './module_bindings';

// ── Types inferred from generated bindings ──────────────────────────────────
type UserRow = { identity: any; name: string; online: boolean; lastSeen: any };
type RoomRow = { id: bigint; name: string; createdBy: any; createdAt: any };
type RoomMemberRow = { id: bigint; roomId: bigint; identity: any; joinedAt: any };
type MessageRow = { id: bigint; roomId: bigint; senderId: any; text: string; sentAt: any };
type TypingIndicatorRow = { id: bigint; roomId: bigint; identity: any; updatedAt: any };
type ReadReceiptRow = { id: bigint; roomId: bigint; identity: any; lastReadMessageId: bigint; readAt: any };

// ── Helpers ──────────────────────────────────────────────────────────────────
function identityStr(id: any): string {
  return id?.toHexString?.() ?? String(id);
}

function tsToMs(ts: any): number {
  if (!ts?.microsSinceUnixEpoch) return 0;
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function formatTime(ts: any): string {
  const d = new Date(tsToMs(ts));
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ── Main App ─────────────────────────────────────────────────────────────────
export default function App() {
  const { getConnection, isActive } = useSpacetimeDB();
  const conn = getConnection();

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.room_member);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typing_indicator);
  const [readReceipts] = useTable(tables.read_receipt);

  const [myIdentity, setMyIdentity] = useState<string | null>(null);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [newRoomInput, setNewRoomInput] = useState('');
  const [messageInput, setMessageInput] = useState('');
  const [view, setView] = useState<'login' | 'chat'>('login');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);

  // Get my identity on connect
  useEffect(() => {
    if (!conn) return;
    const id = conn.identity;
    if (id) {
      setMyIdentity(identityStr(id));
    }
  }, [conn, isActive]);

  // Get my user record
  const myUser = useMemo(
    () => users.find(u => identityStr(u.identity) === myIdentity),
    [users, myIdentity]
  );

  // If we have a name already, go to chat view
  useEffect(() => {
    if (myUser?.name && view === 'login') {
      setView('chat');
    }
  }, [myUser, view]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!selectedRoomId || !conn) return;
    const roomMessages = messages
      .filter(m => m.roomId === selectedRoomId)
      .sort((a, b) => (a.id < b.id ? -1 : 1));
    if (roomMessages.length === 0) return;
    const lastMsg = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [selectedRoomId, messages.length]);

  // ── Derived state ──────────────────────────────────────────────────────────
  const myRoomIds = useMemo(
    () => new Set(roomMembers.filter(m => identityStr(m.identity) === myIdentity).map(m => m.roomId)),
    [roomMembers, myIdentity]
  );

  const roomMessages = useMemo(
    () => messages.filter(m => m.roomId === selectedRoomId).sort((a, b) => (a.id < b.id ? -1 : 1)),
    [messages, selectedRoomId]
  );

  // Typing users in current room (exclude self, filter by age < 5s)
  const typingUsers = useMemo(() => {
    if (!selectedRoomId) return [];
    const now = Date.now();
    return typingIndicators
      .filter(ti => {
        if (ti.roomId !== selectedRoomId) return false;
        if (identityStr(ti.identity) === myIdentity) return false;
        const age = now - tsToMs(ti.updatedAt);
        return age < 5000;
      })
      .map(ti => users.find(u => identityStr(u.identity) === identityStr(ti.identity))?.name ?? 'Someone');
  }, [typingIndicators, selectedRoomId, myIdentity, users]);

  // Unread counts per room
  const unreadCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const room of rooms) {
      const rid = room.id;
      const roomMsgs = messages.filter(m => m.roomId === rid);
      const receipt = readReceipts.find(
        rr => rr.roomId === rid && identityStr(rr.identity) === myIdentity
      );
      const lastRead = receipt?.lastReadMessageId ?? -1n;
      const unread = roomMsgs.filter(m => m.id > lastRead).length;
      counts[rid.toString()] = unread;
    }
    return counts;
  }, [rooms, messages, readReceipts, myIdentity]);

  // Seen-by for a message
  function seenByNames(messageId: bigint, roomId: bigint): string[] {
    return readReceipts
      .filter(rr => rr.roomId === roomId && rr.lastReadMessageId >= messageId && identityStr(rr.identity) !== myIdentity)
      .map(rr => users.find(u => identityStr(u.identity) === identityStr(rr.identity))?.name ?? 'Someone');
  }

  // ── Handlers ───────────────────────────────────────────────────────────────
  function handleSetName(e: React.FormEvent) {
    e.preventDefault();
    if (!conn || !nameInput.trim()) return;
    conn.reducers.setName({ name: nameInput.trim() });
    setView('chat');
  }

  function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    if (!conn || !newRoomInput.trim()) return;
    conn.reducers.createRoom({ name: newRoomInput.trim() });
    setNewRoomInput('');
  }

  function handleJoinRoom(roomId: bigint) {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  }

  function handleLeaveRoom(roomId: bigint) {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageInput.trim() });
    setMessageInput('');
    // Clear typing
    if (isTypingRef.current) {
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
      isTypingRef.current = false;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
  }

  function handleMessageInputChange(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageInput(e.target.value);
    if (!conn || !selectedRoomId) return;

    if (!isTypingRef.current) {
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: true });
      isTypingRef.current = true;
    }

    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      if (conn && selectedRoomId) {
        conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
      }
      isTypingRef.current = false;
    }, 3000);
  }

  // ── Render ─────────────────────────────────────────────────────────────────
  if (!isActive || !conn) {
    return <div className="loading">Connecting to SpacetimeDB...</div>;
  }

  if (view === 'login' || !myUser?.name) {
    return (
      <div className="login-screen">
        <div className="login-box">
          <h1>SpacetimeDB Chat</h1>
          <form onSubmit={handleSetName}>
            <input
              type="text"
              placeholder="Enter your display name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              maxLength={32}
              autoFocus
            />
            <button type="submit">Join Chat</button>
          </form>
        </div>
      </div>
    );
  }

  const onlineUsers = users.filter(u => u.online && u.name);

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <span className="my-name">{myUser.name}</span>
          <span className="online-dot" title="online">●</span>
        </div>

        {/* Online users */}
        <div className="section">
          <div className="section-title">Online ({onlineUsers.length})</div>
          <ul className="user-list">
            {onlineUsers.map(u => (
              <li key={identityStr(u.identity)} className="user-item">
                <span className="dot online">●</span>
                {u.name}
              </li>
            ))}
          </ul>
        </div>

        {/* Rooms */}
        <div className="section">
          <div className="section-title">Rooms</div>
          <ul className="room-list">
            {rooms.map(room => {
              const isMember = myRoomIds.has(room.id);
              const unread = unreadCounts[room.id.toString()] ?? 0;
              return (
                <li
                  key={room.id.toString()}
                  className={`room-item ${selectedRoomId === room.id ? 'active' : ''}`}
                >
                  <span
                    className="room-name"
                    onClick={() => { if (isMember) setSelectedRoomId(room.id); }}
                  >
                    #{room.name}
                    {unread > 0 && isMember && selectedRoomId !== room.id && (
                      <span className="badge">{unread}</span>
                    )}
                  </span>
                  {isMember ? (
                    <button className="btn-sm btn-leave" onClick={() => handleLeaveRoom(room.id)}>Leave</button>
                  ) : (
                    <button className="btn-sm btn-join" onClick={() => handleJoinRoom(room.id)}>Join</button>
                  )}
                </li>
              );
            })}
          </ul>
          <form className="create-room-form" onSubmit={handleCreateRoom}>
            <input
              type="text"
              placeholder="New room name"
              value={newRoomInput}
              onChange={e => setNewRoomInput(e.target.value)}
              maxLength={64}
            />
            <button type="submit">+</button>
          </form>
        </div>
      </aside>

      {/* Main chat area */}
      <main className="chat-area">
        {selectedRoomId ? (
          <>
            <div className="chat-header">
              #{rooms.find(r => r.id === selectedRoomId)?.name ?? '...'}
            </div>
            <div className="messages">
              {roomMessages.map(msg => {
                const sender = users.find(u => identityStr(u.identity) === identityStr(msg.senderId));
                const isMe = identityStr(msg.senderId) === myIdentity;
                const seenBy = seenByNames(msg.id, msg.roomId);
                return (
                  <div key={msg.id.toString()} className={`message ${isMe ? 'mine' : ''}`}>
                    <div className="message-header">
                      <span className="sender">{sender?.name ?? 'Unknown'}</span>
                      <span className="timestamp">{formatTime(msg.sentAt)}</span>
                    </div>
                    <div className="message-text">{msg.text}</div>
                    {seenBy.length > 0 && (
                      <div className="seen-by">Seen by {seenBy.join(', ')}</div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>
            {typingUsers.length > 0 && (
              <div className="typing-indicator">
                {typingUsers.length === 1
                  ? `${typingUsers[0]} is typing...`
                  : `${typingUsers.join(', ')} are typing...`}
              </div>
            )}
            <form className="message-form" onSubmit={handleSendMessage}>
              <input
                type="text"
                placeholder="Type a message..."
                value={messageInput}
                onChange={handleMessageInputChange}
                autoFocus
              />
              <button type="submit">Send</button>
            </form>
          </>
        ) : (
          <div className="no-room">Select a room to start chatting</div>
        )}
      </main>
    </div>
  );
}
