import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, Identity } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

// Types
interface Room {
  id: bigint;
  name: string;
  creatorId: Identity;
  isPrivate: boolean;
  isDm: boolean;
}

interface User {
  identity: Identity;
  name: string | undefined;
  status: string;
  online: boolean;
}

interface Message {
  id: bigint;
  roomId: bigint;
  senderId: Identity;
  content: string;
  createdAt: { microsSinceUnixEpoch: bigint };
  editedAt?: { microsSinceUnixEpoch: bigint };
  parentId?: bigint;
  expiresAt?: { microsSinceUnixEpoch: bigint };
}

function App() {
  // Connection state
  const [conn, setConn] = useState<DbConnection | null>(window.__db_conn);
  const [myIdentity, setMyIdentity] = useState<Identity | null>(
    window.__my_identity
  );

  // UI state
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomPrivate, setNewRoomPrivate] = useState(false);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleContent, setScheduleContent] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [showEphemeralOptions, setShowEphemeralOptions] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showThread, setShowThread] = useState<bigint | null>(null);
  const [replyingTo, setReplyingTo] = useState<bigint | null>(null);
  const [editingMessage, setEditingMessage] = useState<bigint | null>(null);
  const [editContent, setEditContent] = useState('');
  const [showEditHistory, setShowEditHistory] = useState<bigint | null>(null);
  const [showInviteModal, setShowInviteModal] = useState(false);
  const [inviteUsername, setInviteUsername] = useState('');
  const [showDmModal, setShowDmModal] = useState(false);
  const [dmUsername, setDmUsername] = useState('');
  const [showUserMenu, setShowUserMenu] = useState<string | null>(null);
  const [showReactionPicker, setShowReactionPicker] = useState<bigint | null>(
    null
  );
  const [statusDropdown, setStatusDropdown] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastTypingRef = useRef<number>(0);

  // Data from SpacetimeDB
  const [users, usersLoading] = useTable(tables.user);
  const [rooms, roomsLoading] = useTable(tables.room);
  const [roomMembers, membersLoading] = useTable(tables.roomMember);
  const [messages, messagesLoading] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [reactions] = useTable(tables.reaction);
  const [editHistory] = useTable(tables.editHistory);
  const [invitations] = useTable(tables.roomInvitation);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // Poll for connection
  useEffect(() => {
    const interval = setInterval(() => {
      if (window.__db_conn && !conn) setConn(window.__db_conn);
      if (window.__my_identity && !myIdentity)
        setMyIdentity(window.__my_identity);
    }, 100);
    return () => clearInterval(interval);
  }, [conn, myIdentity]);

  // Heartbeat for presence
  useEffect(() => {
    if (!conn) return;
    const interval = setInterval(() => {
      conn.reducers.heartbeat({});
    }, 30000);
    return () => clearInterval(interval);
  }, [conn]);

  // Get current user
  const currentUser = users?.find(
    (u: User) =>
      myIdentity && u.identity.toHexString() === myIdentity.toHexString()
  );

  // Get rooms I'm a member of
  const myRoomIds = new Set(
    roomMembers
      ?.filter(
        m => myIdentity && m.userId.toHexString() === myIdentity.toHexString()
      )
      .map(m => m.roomId.toString()) || []
  );

  const myRooms =
    rooms?.filter((r: Room) => myRoomIds.has(r.id.toString())) || [];
  const publicRooms =
    rooms?.filter(
      (r: Room) => !r.isPrivate && !myRoomIds.has(r.id.toString())
    ) || [];

  // Get selected room data
  const selectedRoom = rooms?.find(
    (r: Room) => selectedRoomId && r.id === selectedRoomId
  );
  const roomMessages =
    messages
      ?.filter(
        (m: Message) =>
          selectedRoomId && m.roomId === selectedRoomId && m.parentId == null
      )
      .sort((a: Message, b: Message) =>
        Number(
          a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
        )
      ) || [];

  const roomMembersList =
    roomMembers?.filter(m => selectedRoomId && m.roomId === selectedRoomId) ||
    [];
  const roomUserIds = new Set(roomMembersList.map(m => m.userId.toHexString()));
  const roomUsers =
    users?.filter((u: User) => roomUserIds.has(u.identity.toHexString())) || [];

  // Typing indicators for current room
  const roomTyping =
    typingIndicators?.filter(
      t =>
        selectedRoomId &&
        t.roomId === selectedRoomId &&
        myIdentity &&
        t.userId.toHexString() !== myIdentity.toHexString()
    ) || [];

  // My pending invitations
  const myInvitations =
    invitations?.filter(
      i =>
        myIdentity &&
        i.inviteeId.toHexString() === myIdentity.toHexString() &&
        i.status === 'pending'
    ) || [];

  // My scheduled messages for current room
  const myScheduled =
    scheduledMessages?.filter(
      s =>
        selectedRoomId &&
        s.roomId === selectedRoomId &&
        myIdentity &&
        s.senderId.toHexString() === myIdentity.toHexString()
    ) || [];

  // Calculate unread counts
  const getUnreadCount = useCallback(
    (roomId: bigint) => {
      const receipt = readReceipts?.find(
        r =>
          r.roomId === roomId &&
          myIdentity &&
          r.userId.toHexString() === myIdentity.toHexString()
      );
      const lastReadId = receipt?.lastReadMessageId || 0n;
      const unread =
        messages?.filter(
          (m: Message) =>
            m.roomId === roomId &&
            m.id > lastReadId &&
            myIdentity &&
            m.senderId.toHexString() !== myIdentity.toHexString()
        ).length || 0;
      return unread;
    },
    [readReceipts, messages, myIdentity]
  );

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [roomMessages.length]);

  // Mark messages as read when viewing room
  useEffect(() => {
    if (!conn || !selectedRoomId || !roomMessages.length) return;
    const lastMessage = roomMessages[roomMessages.length - 1];
    if (lastMessage) {
      conn.reducers.markRead({
        roomId: selectedRoomId,
        messageId: lastMessage.id,
      });
    }
  }, [conn, selectedRoomId, roomMessages.length]);

  // Helpers
  const getUserName = (identity: Identity) => {
    const user = users?.find(
      (u: User) => u.identity.toHexString() === identity.toHexString()
    );
    return user?.name || 'Anonymous';
  };

  const getInitials = (identity: Identity) => {
    const name = getUserName(identity);
    return name
      .split(' ')
      .map((n: string) => n[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  };

  const formatTime = (micros: bigint) => {
    const date = new Date(Number(micros / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatDate = (micros: bigint) => {
    const date = new Date(Number(micros / 1000n));
    return date.toLocaleDateString() + ' ' + date.toLocaleTimeString();
  };

  const getTimeRemaining = (expiresAt: { microsSinceUnixEpoch: bigint }) => {
    const now = Date.now() * 1000;
    const remaining = Number(expiresAt.microsSinceUnixEpoch) - now;
    if (remaining <= 0) return 'Expiring...';
    const seconds = Math.floor(remaining / 1_000_000);
    if (seconds < 60) return `${seconds}s remaining`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m remaining`;
  };

  const getDmName = (room: Room) => {
    if (!room.isDm) return room.name;
    const otherMember = roomMembers?.find(
      m =>
        m.roomId === room.id &&
        myIdentity &&
        m.userId.toHexString() !== myIdentity.toHexString()
    );
    if (otherMember) {
      return getUserName(otherMember.userId);
    }
    return 'DM';
  };

  // Handlers
  const handleSetName = () => {
    if (!conn || !nameInput.trim()) return;
    conn.reducers.setName({ name: nameInput.trim() });
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({
      name: newRoomName.trim(),
      isPrivate: newRoomPrivate,
    });
    setNewRoomName('');
    setNewRoomPrivate(false);
    setShowCreateRoom(false);
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  };

  const handleTyping = () => {
    if (!conn || !selectedRoomId) return;
    const now = Date.now();
    if (now - lastTypingRef.current > 2000) {
      conn.reducers.startTyping({ roomId: selectedRoomId });
      lastTypingRef.current = now;
    }
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      if (conn && selectedRoomId) {
        conn.reducers.stopTyping({ roomId: selectedRoomId });
      }
    }, 3000);
  };

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      parentId: replyingTo ?? undefined,
    });
    setMessageInput('');
    setReplyingTo(null);
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
  };

  const handleSendEphemeral = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendEphemeralMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      durationSeconds: BigInt(ephemeralDuration),
    });
    setMessageInput('');
    setShowEphemeralOptions(false);
  };

  const handleScheduleMessage = () => {
    if (!conn || !selectedRoomId || !scheduleContent.trim() || !scheduleTime)
      return;
    const sendAt = new Date(scheduleTime).getTime() * 1000;
    conn.reducers.scheduleMessage({
      roomId: selectedRoomId,
      content: scheduleContent.trim(),
      sendAtMicros: BigInt(sendAt),
    });
    setScheduleContent('');
    setScheduleTime('');
    setShowScheduleModal(false);
  };

  const handleCancelScheduled = (scheduledId: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  };

  const handleEditMessage = (messageId: bigint) => {
    if (!conn || !editContent.trim()) return;
    conn.reducers.editMessage({ messageId, newContent: editContent.trim() });
    setEditingMessage(null);
    setEditContent('');
  };

  const handleDeleteMessage = (messageId: bigint) => {
    if (!conn) return;
    conn.reducers.deleteMessage({ messageId });
  };

  const handleToggleReaction = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    conn.reducers.toggleReaction({ messageId, emoji });
    setShowReactionPicker(null);
  };

  const handleInviteUser = () => {
    if (!conn || !selectedRoomId || !inviteUsername.trim()) return;
    const targetUser = users?.find(
      (u: User) => u.name === inviteUsername.trim()
    );
    if (!targetUser) {
      alert('User not found');
      return;
    }
    conn.reducers.inviteToRoom({
      roomId: selectedRoomId,
      inviteeIdentity: targetUser.identity.toHexString(),
    });
    setInviteUsername('');
    setShowInviteModal(false);
  };

  const handleRespondInvitation = (invitationId: bigint, accept: boolean) => {
    if (!conn) return;
    conn.reducers.respondToInvitation({ invitationId, accept });
  };

  const handleCreateDm = () => {
    if (!conn || !dmUsername.trim()) return;
    const targetUser = users?.find((u: User) => u.name === dmUsername.trim());
    if (!targetUser) {
      alert('User not found');
      return;
    }
    conn.reducers.createDm({
      targetUserIdentity: targetUser.identity.toHexString(),
    });
    setDmUsername('');
    setShowDmModal(false);
  };

  const handleKickUser = (targetIdentity: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.kickUser({ roomId: selectedRoomId, targetIdentity });
    setShowUserMenu(null);
  };

  const handlePromoteUser = (targetIdentity: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.promoteToAdmin({ roomId: selectedRoomId, targetIdentity });
    setShowUserMenu(null);
  };

  const handleSetStatus = (status: string) => {
    if (!conn) return;
    conn.reducers.setStatus({ status });
    setStatusDropdown(false);
  };

  const isAdmin = (roomId: bigint) => {
    const member = roomMembers?.find(
      m =>
        m.roomId === roomId &&
        myIdentity &&
        m.userId.toHexString() === myIdentity.toHexString()
    );
    return member?.role === 'admin';
  };

  // Get thread replies
  const getThreadReplies = (parentId: bigint) => {
    return (
      messages
        ?.filter((m: Message) => m.parentId === parentId)
        .sort((a: Message, b: Message) =>
          Number(
            a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
          )
        ) || []
    );
  };

  // Get reactions for a message
  const getMessageReactions = (messageId: bigint) => {
    const messageReactions =
      reactions?.filter(r => r.messageId === messageId) || [];
    const grouped: {
      [emoji: string]: {
        count: number;
        users: string[];
        hasMyReaction: boolean;
      };
    } = {};
    for (const r of messageReactions) {
      if (!grouped[r.emoji]) {
        grouped[r.emoji] = { count: 0, users: [], hasMyReaction: false };
      }
      grouped[r.emoji].count++;
      grouped[r.emoji].users.push(getUserName(r.userId));
      if (myIdentity && r.userId.toHexString() === myIdentity.toHexString()) {
        grouped[r.emoji].hasMyReaction = true;
      }
    }
    return grouped;
  };

  // Get read receipts for a message
  const getMessageReadBy = (messageId: bigint) => {
    return (
      readReceipts
        ?.filter(
          r =>
            r.lastReadMessageId >= messageId &&
            myIdentity &&
            r.userId.toHexString() !== myIdentity.toHexString() &&
            roomUserIds.has(r.userId.toHexString())
        )
        .map(r => getUserName(r.userId)) || []
    );
  };

  // Loading state
  if (
    !conn ||
    !myIdentity ||
    usersLoading ||
    roomsLoading ||
    membersLoading ||
    messagesLoading
  ) {
    return (
      <div className="loading">
        <div className="loading-spinner"></div>
      </div>
    );
  }

  // Name setup
  if (!currentUser?.name) {
    return (
      <div className="name-setup">
        <div className="name-setup-card">
          <h2>Welcome to Chat App</h2>
          <p>Choose a display name to get started</p>
          <input
            type="text"
            placeholder="Enter your name"
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSetName()}
            maxLength={50}
          />
          <button
            className="btn btn-primary"
            onClick={handleSetName}
            disabled={!nameInput.trim()}
          >
            Continue
          </button>
        </div>
      </div>
    );
  }

  // Render message component
  const renderMessage = (msg: Message, isThread = false) => {
    const msgReactions = getMessageReactions(msg.id);
    const readBy = getMessageReadBy(msg.id);
    const threadReplies = getThreadReplies(msg.id);
    const msgEditHistory =
      editHistory?.filter(e => e.messageId === msg.id) || [];
    const isEphemeral = msg.expiresAt != null;
    const isOwn =
      myIdentity && msg.senderId.toHexString() === myIdentity.toHexString();

    return (
      <div
        key={msg.id.toString()}
        className={`message-group ${isEphemeral ? 'message-ephemeral' : ''}`}
        style={{ position: 'relative' }}
      >
        <div className="message-avatar">{getInitials(msg.senderId)}</div>
        <div className="message-content">
          <div className="message-header">
            <span className="message-author">{getUserName(msg.senderId)}</span>
            <span className="message-time">
              {formatTime(msg.createdAt.microsSinceUnixEpoch)}
            </span>
            {msg.editedAt && <span className="message-edited">(edited)</span>}
          </div>

          {editingMessage === msg.id ? (
            <div style={{ display: 'flex', gap: '8px' }}>
              <input
                type="text"
                className="message-input"
                value={editContent}
                onChange={e => setEditContent(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleEditMessage(msg.id)}
              />
              <button
                className="btn btn-primary"
                onClick={() => handleEditMessage(msg.id)}
              >
                Save
              </button>
              <button
                className="btn btn-secondary"
                onClick={() => setEditingMessage(null)}
              >
                Cancel
              </button>
            </div>
          ) : (
            <p className="message-text">{msg.content}</p>
          )}

          {isEphemeral && (
            <div className="ephemeral-indicator">
              ⏱️ {getTimeRemaining(msg.expiresAt!)}
            </div>
          )}

          {/* Reactions */}
          <div className="reactions-container">
            {Object.entries(msgReactions).map(([emoji, data]) => (
              <button
                key={emoji}
                className={`reaction-button ${data.hasMyReaction ? 'active' : ''}`}
                onClick={() => handleToggleReaction(msg.id, emoji)}
                title={data.users.join(', ')}
              >
                {emoji} <span className="reaction-count">{data.count}</span>
              </button>
            ))}
            <button
              className="add-reaction-btn"
              onClick={() =>
                setShowReactionPicker(
                  showReactionPicker === msg.id ? null : msg.id
                )
              }
            >
              +
            </button>
            {showReactionPicker === msg.id && (
              <div className="reaction-picker">
                {['👍', '❤️', '😂', '😮', '😢', '🎉', '🔥', '👀'].map(emoji => (
                  <button
                    key={emoji}
                    onClick={() => handleToggleReaction(msg.id, emoji)}
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Thread indicator */}
          {!isThread && threadReplies.length > 0 && (
            <div
              className="message-thread-indicator"
              onClick={() => setShowThread(msg.id)}
            >
              💬 {threadReplies.length}{' '}
              {threadReplies.length === 1 ? 'reply' : 'replies'}
            </div>
          )}

          {/* Read receipts */}
          {readBy.length > 0 && (
            <div className="read-receipts">
              Seen by {readBy.slice(0, 3).join(', ')}
              {readBy.length > 3 ? ` +${readBy.length - 3}` : ''}
            </div>
          )}
        </div>

        {/* Message actions */}
        <div className="message-actions">
          {!isThread && (
            <button
              className="action-btn"
              onClick={() => setReplyingTo(msg.id)}
              title="Reply"
            >
              ↩️
            </button>
          )}
          {isOwn && (
            <>
              <button
                className="action-btn"
                onClick={() => {
                  setEditingMessage(msg.id);
                  setEditContent(msg.content);
                }}
                title="Edit"
              >
                ✏️
              </button>
              <button
                className="action-btn"
                onClick={() => handleDeleteMessage(msg.id)}
                title="Delete"
              >
                🗑️
              </button>
            </>
          )}
          {msgEditHistory.length > 0 && (
            <button
              className="action-btn"
              onClick={() => setShowEditHistory(msg.id)}
              title="View history"
            >
              📜
            </button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="app-container">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>💬 Chat App</h2>
          <div className="dropdown">
            <button
              className="btn-icon"
              onClick={() => setStatusDropdown(!statusDropdown)}
            >
              ⚙️
            </button>
            {statusDropdown && (
              <div className="dropdown-menu">
                <div
                  className="dropdown-item"
                  onClick={() => handleSetStatus('online')}
                >
                  <span
                    className="status-indicator status-online"
                    style={{ position: 'static' }}
                  ></span>{' '}
                  Online
                </div>
                <div
                  className="dropdown-item"
                  onClick={() => handleSetStatus('away')}
                >
                  <span
                    className="status-indicator status-away"
                    style={{ position: 'static' }}
                  ></span>{' '}
                  Away
                </div>
                <div
                  className="dropdown-item"
                  onClick={() => handleSetStatus('dnd')}
                >
                  <span
                    className="status-indicator status-dnd"
                    style={{ position: 'static' }}
                  ></span>{' '}
                  Do Not Disturb
                </div>
                <div
                  className="dropdown-item"
                  onClick={() => handleSetStatus('invisible')}
                >
                  <span
                    className="status-indicator status-invisible"
                    style={{ position: 'static' }}
                  ></span>{' '}
                  Invisible
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="room-list">
          {/* Pending invitations */}
          {myInvitations.length > 0 && (
            <div className="room-section">
              <div className="room-section-title">📩 Invitations</div>
              {myInvitations.map(inv => {
                const room = rooms?.find((r: Room) => r.id === inv.roomId);
                return (
                  <div key={inv.id.toString()} className="invitation-item">
                    <div className="invitation-info">
                      <strong>{room?.name || 'Unknown Room'}</strong>
                      <div
                        style={{
                          fontSize: '0.8rem',
                          color: 'var(--text-secondary)',
                        }}
                      >
                        from {getUserName(inv.inviterId)}
                      </div>
                    </div>
                    <div className="invitation-actions">
                      <button
                        className="btn btn-primary"
                        style={{ padding: '4px 8px', fontSize: '0.8rem' }}
                        onClick={() => handleRespondInvitation(inv.id, true)}
                      >
                        ✓
                      </button>
                      <button
                        className="btn btn-secondary"
                        style={{ padding: '4px 8px', fontSize: '0.8rem' }}
                        onClick={() => handleRespondInvitation(inv.id, false)}
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          {/* DMs */}
          <div className="room-section">
            <div
              className="room-section-title"
              style={{ display: 'flex', justifyContent: 'space-between' }}
            >
              Direct Messages
              <button
                className="btn-icon"
                style={{ padding: '0', fontSize: '0.9rem' }}
                onClick={() => setShowDmModal(true)}
              >
                +
              </button>
            </div>
            {myRooms
              .filter((r: Room) => r.isDm)
              .map((room: Room) => {
                const unread = getUnreadCount(room.id);
                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item ${selectedRoomId === room.id ? 'active' : ''}`}
                    onClick={() => setSelectedRoomId(room.id)}
                  >
                    <div className="room-name">
                      <span className="room-icon">👤</span>
                      {getDmName(room)}
                    </div>
                    {unread > 0 && (
                      <span className="unread-badge">{unread}</span>
                    )}
                  </div>
                );
              })}
          </div>

          {/* My Rooms */}
          <div className="room-section">
            <div
              className="room-section-title"
              style={{ display: 'flex', justifyContent: 'space-between' }}
            >
              My Rooms
              <button
                className="btn-icon"
                style={{ padding: '0', fontSize: '0.9rem' }}
                onClick={() => setShowCreateRoom(true)}
              >
                +
              </button>
            </div>
            {myRooms
              .filter((r: Room) => !r.isDm)
              .map((room: Room) => {
                const unread = getUnreadCount(room.id);
                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item ${selectedRoomId === room.id ? 'active' : ''}`}
                    onClick={() => setSelectedRoomId(room.id)}
                  >
                    <div className="room-name">
                      <span className="room-icon">
                        {room.isPrivate ? '🔒' : '#'}
                      </span>
                      {room.name}
                    </div>
                    {unread > 0 && (
                      <span className="unread-badge">{unread}</span>
                    )}
                  </div>
                );
              })}
          </div>

          {/* Public Rooms */}
          {publicRooms.length > 0 && (
            <div className="room-section">
              <div className="room-section-title">Browse Rooms</div>
              {publicRooms.map((room: Room) => (
                <div
                  key={room.id.toString()}
                  className="room-item"
                  onClick={() => handleJoinRoom(room.id)}
                >
                  <div className="room-name">
                    <span className="room-icon">#</span>
                    {room.name}
                  </div>
                  <span
                    style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}
                  >
                    Join
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Main Content */}
      <div className="main-content">
        {selectedRoom ? (
          <>
            <div className="chat-header">
              <div>
                <h3>
                  {selectedRoom.isPrivate ? '🔒' : '#'}{' '}
                  {selectedRoom.isDm
                    ? getDmName(selectedRoom)
                    : selectedRoom.name}
                </h3>
              </div>
              <div className="header-actions">
                {selectedRoom.isPrivate &&
                  !selectedRoom.isDm &&
                  isAdmin(selectedRoom.id) && (
                    <button
                      className="btn btn-secondary"
                      onClick={() => setShowInviteModal(true)}
                    >
                      Invite
                    </button>
                  )}
                <button
                  className="btn btn-secondary"
                  onClick={() => setShowScheduleModal(true)}
                >
                  Schedule
                </button>
                {!selectedRoom.isDm && (
                  <button
                    className="btn btn-secondary"
                    onClick={() => handleLeaveRoom(selectedRoom.id)}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            {/* Scheduled messages */}
            {myScheduled.length > 0 && (
              <div
                className="scheduled-messages-panel"
                style={{ margin: '0 20px', marginTop: '12px' }}
              >
                <div
                  style={{
                    fontSize: '0.8rem',
                    fontWeight: '600',
                    marginBottom: '8px',
                  }}
                >
                  📅 Scheduled Messages
                </div>
                {myScheduled.map(s => (
                  <div
                    key={s.scheduledId.toString()}
                    className="scheduled-message-item"
                  >
                    <div>
                      <div>{s.content}</div>
                      <div className="scheduled-time">
                        Sends at:{' '}
                        {formatDate(
                          s.scheduledAt.time?.microsSinceUnixEpoch || 0n
                        )}
                      </div>
                    </div>
                    <button
                      className="btn-icon"
                      onClick={() => handleCancelScheduled(s.scheduledId)}
                      title="Cancel"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            )}

            <div className="messages-container">
              {roomMessages.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-state-icon">💬</div>
                  <h4>No messages yet</h4>
                  <p>Be the first to send a message!</p>
                </div>
              ) : (
                roomMessages.map((msg: Message) => renderMessage(msg))
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {roomTyping.length > 0 && (
              <div className="typing-indicator">
                <div className="typing-dots">
                  <span></span>
                  <span></span>
                  <span></span>
                </div>
                {roomTyping.length === 1
                  ? `${getUserName(roomTyping[0].userId)} is typing...`
                  : `${roomTyping.length} people are typing...`}
              </div>
            )}

            {/* Reply indicator */}
            {replyingTo && (
              <div
                style={{
                  padding: '8px 20px',
                  background: 'var(--bg-tertiary)',
                  display: 'flex',
                  justifyContent: 'space-between',
                }}
              >
                <span>
                  Replying to{' '}
                  {getUserName(
                    messages?.find((m: Message) => m.id === replyingTo)
                      ?.senderId!
                  )}
                </span>
                <button
                  className="btn-icon"
                  onClick={() => setReplyingTo(null)}
                >
                  ✕
                </button>
              </div>
            )}

            <div className="message-input-container">
              <div className="message-input-wrapper">
                <input
                  type="text"
                  className="message-input"
                  placeholder={
                    replyingTo ? 'Reply to message...' : 'Type a message...'
                  }
                  value={messageInput}
                  onChange={e => {
                    setMessageInput(e.target.value);
                    handleTyping();
                  }}
                  onKeyDown={e =>
                    e.key === 'Enter' && !e.shiftKey && handleSendMessage()
                  }
                />
                <div className="input-actions">
                  <div className="dropdown">
                    <button
                      className="btn-icon"
                      onClick={() =>
                        setShowEphemeralOptions(!showEphemeralOptions)
                      }
                      title="Ephemeral message"
                    >
                      ⏱️
                    </button>
                    {showEphemeralOptions && (
                      <div
                        className="dropdown-menu"
                        style={{ bottom: '100%', top: 'auto' }}
                      >
                        <div
                          className="dropdown-item"
                          onClick={() => {
                            setEphemeralDuration(60);
                            handleSendEphemeral();
                          }}
                        >
                          1 minute
                        </div>
                        <div
                          className="dropdown-item"
                          onClick={() => {
                            setEphemeralDuration(300);
                            handleSendEphemeral();
                          }}
                        >
                          5 minutes
                        </div>
                        <div
                          className="dropdown-item"
                          onClick={() => {
                            setEphemeralDuration(600);
                            handleSendEphemeral();
                          }}
                        >
                          10 minutes
                        </div>
                      </div>
                    )}
                  </div>
                  <button
                    className="btn btn-primary"
                    onClick={handleSendMessage}
                    disabled={!messageInput.trim()}
                  >
                    Send
                  </button>
                </div>
              </div>
            </div>
          </>
        ) : (
          <div className="empty-state">
            <div className="empty-state-icon">👋</div>
            <h4>Welcome, {currentUser.name}!</h4>
            <p>Select a room to start chatting or create a new one</p>
          </div>
        )}
      </div>

      {/* Users Panel */}
      {selectedRoom && (
        <div className="users-panel">
          <h4>Members — {roomUsers.length}</h4>
          {roomUsers
            .sort((a: User, b: User) => (b.online ? 1 : 0) - (a.online ? 1 : 0))
            .map((user: User) => {
              const member = roomMembersList.find(
                m => m.userId.toHexString() === user.identity.toHexString()
              );
              const isUserAdmin = member?.role === 'admin';
              const isSelf =
                myIdentity &&
                user.identity.toHexString() === myIdentity.toHexString();

              return (
                <div
                  key={user.identity.toHexString()}
                  className="user-item"
                  onClick={() =>
                    !isSelf &&
                    setShowUserMenu(
                      showUserMenu === user.identity.toHexString()
                        ? null
                        : user.identity.toHexString()
                    )
                  }
                  style={{ position: 'relative' }}
                >
                  <div className="user-avatar">
                    {getInitials(user.identity)}
                    <span
                      className={`status-indicator status-${user.status === 'invisible' ? 'invisible' : user.online ? user.status : 'invisible'}`}
                    ></span>
                  </div>
                  <div className="user-info">
                    <div className="user-name">
                      {user.name || 'Anonymous'}
                      {isUserAdmin && (
                        <span
                          style={{ marginLeft: '4px', fontSize: '0.75rem' }}
                        >
                          👑
                        </span>
                      )}
                    </div>
                    <div className="user-status-text">
                      {user.online
                        ? user.status
                        : `Last seen ${formatTime(user.lastActive.microsSinceUnixEpoch)}`}
                    </div>
                  </div>

                  {showUserMenu === user.identity.toHexString() &&
                    isAdmin(selectedRoom.id) &&
                    !isSelf && (
                      <div
                        className="dropdown-menu"
                        style={{
                          position: 'absolute',
                          top: '100%',
                          left: '0',
                          zIndex: 100,
                        }}
                      >
                        {!isUserAdmin && (
                          <div
                            className="dropdown-item"
                            onClick={() =>
                              handlePromoteUser(user.identity.toHexString())
                            }
                          >
                            👑 Make Admin
                          </div>
                        )}
                        <div
                          className="dropdown-item danger"
                          onClick={() =>
                            handleKickUser(user.identity.toHexString())
                          }
                        >
                          🚫 Kick
                        </div>
                      </div>
                    )}
                </div>
              );
            })}
        </div>
      )}

      {/* Thread Panel */}
      {showThread && (
        <div className="thread-panel">
          <div className="thread-header">
            <h4>Thread</h4>
            <button className="btn-icon" onClick={() => setShowThread(null)}>
              ✕
            </button>
          </div>
          <div className="thread-messages">
            {/* Parent message */}
            <div className="parent-message">
              {renderMessage(
                messages?.find((m: Message) => m.id === showThread)!,
                true
              )}
            </div>

            <div className="thread-divider">
              <hr />
              <span>{getThreadReplies(showThread).length} replies</span>
              <hr />
            </div>

            {/* Replies */}
            {getThreadReplies(showThread).map((msg: Message) =>
              renderMessage(msg, true)
            )}
          </div>

          <div className="message-input-container">
            <div className="message-input-wrapper">
              <input
                type="text"
                className="message-input"
                placeholder="Reply in thread..."
                value={messageInput}
                onChange={e => setMessageInput(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter') {
                    if (conn && selectedRoomId && messageInput.trim()) {
                      conn.reducers.sendMessage({
                        roomId: selectedRoomId,
                        content: messageInput.trim(),
                        parentId: showThread,
                      });
                      setMessageInput('');
                    }
                  }
                }}
              />
              <button
                className="btn btn-primary"
                onClick={() => {
                  if (conn && selectedRoomId && messageInput.trim()) {
                    conn.reducers.sendMessage({
                      roomId: selectedRoomId,
                      content: messageInput.trim(),
                      parentId: showThread,
                    });
                    setMessageInput('');
                  }
                }}
              >
                Reply
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Create Room Modal */}
      {showCreateRoom && (
        <div className="modal-overlay" onClick={() => setShowCreateRoom(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Create Room</h3>
            <input
              type="text"
              className="modal-input"
              placeholder="Room name"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
            />
            <div className="checkbox-group">
              <input
                type="checkbox"
                id="privateRoom"
                checked={newRoomPrivate}
                onChange={e => setNewRoomPrivate(e.target.checked)}
              />
              <label htmlFor="privateRoom">Private (invite-only)</label>
            </div>
            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowCreateRoom(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleCreateRoom}
                disabled={!newRoomName.trim()}
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Schedule Message Modal */}
      {showScheduleModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowScheduleModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Schedule Message</h3>
            <textarea
              className="modal-input"
              placeholder="Message content"
              value={scheduleContent}
              onChange={e => setScheduleContent(e.target.value)}
              rows={3}
            />
            <input
              type="datetime-local"
              className="modal-input"
              value={scheduleTime}
              onChange={e => setScheduleTime(e.target.value)}
              min={new Date().toISOString().slice(0, 16)}
            />
            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowScheduleModal(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleScheduleMessage}
                disabled={!scheduleContent.trim() || !scheduleTime}
              >
                Schedule
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Invite Modal */}
      {showInviteModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowInviteModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Invite User</h3>
            <input
              type="text"
              className="modal-input"
              placeholder="Username"
              value={inviteUsername}
              onChange={e => setInviteUsername(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleInviteUser()}
            />
            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowInviteModal(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleInviteUser}
                disabled={!inviteUsername.trim()}
              >
                Invite
              </button>
            </div>
          </div>
        </div>
      )}

      {/* DM Modal */}
      {showDmModal && (
        <div className="modal-overlay" onClick={() => setShowDmModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>New Direct Message</h3>
            <input
              type="text"
              className="modal-input"
              placeholder="Username"
              value={dmUsername}
              onChange={e => setDmUsername(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateDm()}
            />
            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowDmModal(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleCreateDm}
                disabled={!dmUsername.trim()}
              >
                Start Chat
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit History Modal */}
      {showEditHistory && (
        <div className="modal-overlay" onClick={() => setShowEditHistory(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            <div className="edit-history-list">
              {editHistory
                ?.filter(e => e.messageId === showEditHistory)
                .sort((a, b) =>
                  Number(
                    b.editedAt.microsSinceUnixEpoch -
                      a.editedAt.microsSinceUnixEpoch
                  )
                )
                .map(edit => (
                  <div key={edit.id.toString()} className="edit-history-item">
                    <div className="edit-history-time">
                      {formatDate(edit.editedAt.microsSinceUnixEpoch)}
                    </div>
                    <div className="edit-history-content">
                      {edit.oldContent}
                    </div>
                  </div>
                ))}
            </div>
            <div className="modal-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowEditHistory(null)}
              >
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
