import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, Identity } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

// Types for our data
type User = typeof tables.user extends { rowType: infer R } ? R : never;
type Room = typeof tables.room extends { rowType: infer R } ? R : never;
type RoomMember = typeof tables.roomMember extends { rowType: infer R }
  ? R
  : never;
type Message = typeof tables.message extends { rowType: infer R } ? R : never;
type Reaction = typeof tables.reaction extends { rowType: infer R } ? R : never;
type MessageEdit = typeof tables.messageEdit extends { rowType: infer R }
  ? R
  : never;
type ReadReceipt = typeof tables.readReceipt extends { rowType: infer R }
  ? R
  : never;
type RoomInvitation = typeof tables.roomInvitation extends { rowType: infer R }
  ? R
  : never;

// Available emoji reactions
const EMOJI_OPTIONS = ['👍', '❤️', '😂', '😮', '😢', '🔥', '🎉', '💯'];

function App() {
  // Connection state
  const [conn, setConn] = useState<DbConnection | null>(window.__db_conn);
  const [myIdentity, setMyIdentity] = useState<Identity | null>(
    window.__my_identity
  );

  // UI state
  const [displayName, setDisplayName] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleDateTime, setScheduleDateTime] = useState('');
  const [showCreateRoomModal, setShowCreateRoomModal] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomPrivate, setNewRoomPrivate] = useState(false);
  const [showInviteModal, setShowInviteModal] = useState(false);
  const [inviteUsername, setInviteUsername] = useState('');
  const [showDmModal, setShowDmModal] = useState(false);
  const [dmUsername, setDmUsername] = useState('');
  const [replyingTo, setReplyingTo] = useState<bigint | null>(null);
  const [viewingThread, setViewingThread] = useState<bigint | null>(null);
  const [showEditHistoryModal, setShowEditHistoryModal] = useState<
    bigint | null
  >(null);
  const [editingMessage, setEditingMessage] = useState<bigint | null>(null);
  const [editContent, setEditContent] = useState('');
  const [showMembersModal, setShowMembersModal] = useState(false);
  const [showReactionPicker, setShowReactionPicker] = useState<bigint | null>(
    null
  );
  const [, setNow] = useState(Date.now());

  // Typing indicator timeout ref
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const isNearBottomRef = useRef(true);

  // Data from SpacetimeDB
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [reactions] = useTable(tables.reaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [readReceipts] = useTable(tables.readReceipt);
  const [invitations] = useTable(tables.roomInvitation);
  const [typingIndicatorsRaw] = useTable(tables.typingIndicator);
  const [scheduledMessagesRaw] = useTable(tables.scheduledMessage);

  // Poll for connection
  useEffect(() => {
    if (window.__db_conn && !conn) setConn(window.__db_conn);
    if (window.__my_identity && !myIdentity)
      setMyIdentity(window.__my_identity);
    const interval = setInterval(() => {
      if (window.__db_conn && !conn) setConn(window.__db_conn);
      if (window.__my_identity && !myIdentity)
        setMyIdentity(window.__my_identity);
    }, 100);
    return () => clearInterval(interval);
  }, [conn, myIdentity]);

  // Update current time for countdowns
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  // Current user
  const currentUser = users?.find(
    u => myIdentity && u.identity.toHexString() === myIdentity.toHexString()
  );

  // Check if user has set a name
  const hasName = currentUser?.name != null && currentUser.name.trim() !== '';

  // Get rooms the user is a member of
  const myMemberships =
    roomMembers?.filter(
      m =>
        myIdentity &&
        m.userIdentity.toHexString() === myIdentity.toHexString() &&
        !m.isBanned
    ) ?? [];

  const myRoomIds = new Set(myMemberships.map(m => m.roomId));

  // Categorize rooms
  const publicRooms = rooms?.filter(r => !r.isPrivate) ?? [];
  const myPrivateRooms =
    rooms?.filter(r => r.isPrivate && !r.isDm && myRoomIds.has(r.id)) ?? [];
  const myDmRooms = rooms?.filter(r => r.isDm && myRoomIds.has(r.id)) ?? [];

  // Selected room data
  const selectedRoom = rooms?.find(r => r.id === selectedRoomId);
  const selectedRoomMembership = myMemberships.find(
    m => m.roomId === selectedRoomId
  );
  const isRoomMember = selectedRoomMembership != null;
  const isRoomAdmin = selectedRoomMembership?.isAdmin ?? false;

  // Messages for selected room
  const roomMessages =
    messages
      ?.filter(m => m.roomId === selectedRoomId)
      .sort((a, b) =>
        Number(
          a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
        )
      ) ?? [];

  // Top-level messages (not replies)
  const topLevelMessages = viewingThread
    ? roomMessages.filter(
        m => m.id === viewingThread || m.parentMessageId === viewingThread
      )
    : roomMessages.filter(m => m.parentMessageId == null);

  // Get reply count for a message
  const getReplyCount = useCallback(
    (messageId: bigint) => {
      return messages?.filter(m => m.parentMessageId === messageId).length ?? 0;
    },
    [messages]
  );

  // Pending invitations for current user
  const pendingInvitations =
    invitations?.filter(
      inv =>
        myIdentity &&
        inv.inviteeIdentity.toHexString() === myIdentity.toHexString() &&
        inv.status === 'pending'
    ) ?? [];

  // Typing indicators for selected room
  const typingIndicators =
    typingIndicatorsRaw?.filter(
      t =>
        t.roomId === selectedRoomId &&
        myIdentity &&
        t.userIdentity.toHexString() !== myIdentity.toHexString()
    ) ?? [];

  // Scheduled messages for selected room (filter by current user since table is public)
  const scheduledMessages =
    scheduledMessagesRaw?.filter(
      s =>
        s.roomId === selectedRoomId &&
        myIdentity &&
        s.ownerIdentity.toHexString() === myIdentity.toHexString()
    ) ?? [];

  // Unread counts per room
  const getUnreadCount = useCallback(
    (roomId: bigint) => {
      const membership = myMemberships.find(m => m.roomId === roomId);
      if (!membership) return 0;
      const lastRead = membership.lastReadMessageId;
      return (
        messages?.filter(
          m => m.roomId === roomId && (lastRead == null || m.id > lastRead)
        ).length ?? 0
      );
    },
    [messages, myMemberships]
  );

  // Online users
  const onlineUsers =
    users?.filter(u => u.online && u.status !== 'invisible') ?? [];

  // Room members for current room
  const currentRoomMembers =
    roomMembers?.filter(m => m.roomId === selectedRoomId && !m.isBanned) ?? [];

  // Helper to get user name
  const getUserName = useCallback(
    (identity: Identity) => {
      const user = users?.find(
        u => u.identity.toHexString() === identity.toHexString()
      );
      return user?.name ?? 'Unknown';
    },
    [users]
  );

  // Helper to get user initials
  const getInitials = (name: string | undefined) => {
    if (!name) return '?';
    return name.charAt(0).toUpperCase();
  };

  // Helper to format timestamp
  const formatTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Helper to format relative time
  const formatRelativeTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const now = Date.now();
    const then = Number(timestamp.microsSinceUnixEpoch / 1000n);
    const diff = now - then;
    const minutes = Math.floor(diff / 60000);
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  };

  // Helper for countdown display
  const formatCountdown = (expiresAt: { microsSinceUnixEpoch: bigint }) => {
    const now = Date.now() * 1000;
    const expires = Number(expiresAt.microsSinceUnixEpoch);
    const remaining = Math.max(0, Math.floor((expires - now) / 1000000));
    if (remaining <= 0) return 'expiring...';
    if (remaining < 60) return `${remaining}s`;
    return `${Math.floor(remaining / 60)}m ${remaining % 60}s`;
  };

  // Auto-scroll when near bottom
  useEffect(() => {
    if (isNearBottomRef.current && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [roomMessages.length]);

  // Handle scroll to check if near bottom
  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const target = e.target as HTMLDivElement;
    const threshold = 100;
    isNearBottomRef.current =
      target.scrollHeight - target.scrollTop - target.clientHeight < threshold;
  };

  // Mark room as read when viewing
  useEffect(() => {
    if (selectedRoomId && isRoomMember && conn) {
      conn.reducers.markRoomRead({ roomId: selectedRoomId });
    }
  }, [selectedRoomId, isRoomMember, conn, roomMessages.length]);

  // Handlers
  const handleSetName = () => {
    if (!conn || !displayName.trim()) return;
    conn.reducers.setName({ name: displayName.trim() });
  };

  const handleSetStatus = (status: string) => {
    if (!conn) return;
    conn.reducers.setStatus({ status });
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({
      name: newRoomName.trim(),
      isPrivate: newRoomPrivate,
    });
    setNewRoomName('');
    setNewRoomPrivate(false);
    setShowCreateRoomModal(false);
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
    setSelectedRoomId(roomId);
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) {
      setSelectedRoomId(null);
    }
  };

  const handleSendMessage = () => {
    if (!conn || !messageInput.trim() || !selectedRoomId) return;
    conn.reducers.sendMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      parentMessageId: replyingTo ?? undefined,
      isEphemeral,
      ephemeralDurationSecs: isEphemeral
        ? BigInt(ephemeralDuration)
        : undefined,
    });
    setMessageInput('');
    setReplyingTo(null);
    setIsEphemeral(false);
  };

  const handleTyping = () => {
    if (!conn || !selectedRoomId) return;

    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    conn.reducers.startTyping({ roomId: selectedRoomId });

    typingTimeoutRef.current = setTimeout(() => {
      conn.reducers.stopTyping({ roomId: selectedRoomId });
    }, 3000);
  };

  const handleScheduleMessage = () => {
    if (!conn || !messageInput.trim() || !selectedRoomId || !scheduleDateTime)
      return;
    const scheduledTime = new Date(scheduleDateTime).getTime();
    conn.reducers.scheduleMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      scheduledTimeMs: BigInt(scheduledTime),
    });
    setMessageInput('');
    setShowScheduleModal(false);
    setScheduleDateTime('');
  };

  const handleCancelScheduled = (scheduledId: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  };

  const handleToggleReaction = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    conn.reducers.toggleReaction({ messageId, emoji });
    setShowReactionPicker(null);
  };

  const handleEditMessage = (messageId: bigint, content: string) => {
    setEditingMessage(messageId);
    setEditContent(content);
  };

  const handleSaveEdit = () => {
    if (!conn || editingMessage == null || !editContent.trim()) return;
    conn.reducers.editMessage({
      messageId: editingMessage,
      newContent: editContent.trim(),
    });
    setEditingMessage(null);
    setEditContent('');
  };

  const handleDeleteMessage = (messageId: bigint) => {
    if (!conn) return;
    conn.reducers.deleteMessage({ messageId });
  };

  const handleInviteUser = () => {
    if (!conn || !inviteUsername.trim() || !selectedRoomId) return;
    conn.reducers.inviteToRoom({
      roomId: selectedRoomId,
      inviteeUsername: inviteUsername.trim(),
    });
    setInviteUsername('');
    setShowInviteModal(false);
  };

  const handleRespondToInvitation = (invitationId: bigint, accept: boolean) => {
    if (!conn) return;
    conn.reducers.respondToInvitation({ invitationId, accept });
  };

  const handleStartDm = () => {
    if (!conn || !dmUsername.trim()) return;
    conn.reducers.startDm({ targetUsername: dmUsername.trim() });
    setDmUsername('');
    setShowDmModal(false);
  };

  const handleKickUser = (username: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.kickUser({
      roomId: selectedRoomId,
      targetUsername: username,
    });
  };

  const handleBanUser = (username: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.banUser({ roomId: selectedRoomId, targetUsername: username });
  };

  const handlePromoteUser = (username: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.promoteToAdmin({
      roomId: selectedRoomId,
      targetUsername: username,
    });
  };

  // Get reactions for a message grouped by emoji
  const getMessageReactions = useCallback(
    (messageId: bigint) => {
      const msgReactions =
        reactions?.filter(r => r.messageId === messageId) ?? [];
      const grouped = new Map<
        string,
        { count: number; users: string[]; userReacted: boolean }
      >();

      for (const r of msgReactions) {
        const current = grouped.get(r.emoji) ?? {
          count: 0,
          users: [],
          userReacted: false,
        };
        current.count++;
        current.users.push(getUserName(r.userIdentity));
        if (
          myIdentity &&
          r.userIdentity.toHexString() === myIdentity.toHexString()
        ) {
          current.userReacted = true;
        }
        grouped.set(r.emoji, current);
      }

      return grouped;
    },
    [reactions, getUserName, myIdentity]
  );

  // Get read receipts for a message
  const getReadReceipts = useCallback(
    (messageId: bigint) => {
      return (
        readReceipts
          ?.filter(
            r =>
              r.messageId === messageId &&
              myIdentity &&
              r.userIdentity.toHexString() !== myIdentity.toHexString()
          )
          .map(r => getUserName(r.userIdentity)) ?? []
      );
    },
    [readReceipts, getUserName, myIdentity]
  );

  // Get edit history for a message
  const getEditHistory = useCallback(
    (messageId: bigint) => {
      return (
        messageEdits
          ?.filter(e => e.messageId === messageId)
          .sort((a, b) =>
            Number(
              b.editedAt.microsSinceUnixEpoch - a.editedAt.microsSinceUnixEpoch
            )
          ) ?? []
      );
    },
    [messageEdits]
  );

  // Get DM partner name
  const getDmPartnerName = (room: Room) => {
    if (!room.isDm) return room.name;
    const members = roomMembers?.filter(m => m.roomId === room.id) ?? [];
    const partner = members.find(
      m =>
        myIdentity && m.userIdentity.toHexString() !== myIdentity.toHexString()
    );
    return partner ? getUserName(partner.userIdentity) : 'Unknown';
  };

  // Loading state
  if (!conn) {
    return (
      <div className="loading">
        <div className="loading-spinner" />
      </div>
    );
  }

  // Setup screen - set display name
  if (!hasName) {
    return (
      <div className="setup-screen">
        <h1>Welcome to ChatApp</h1>
        <p>Enter your display name to get started</p>
        <div className="setup-form">
          <input
            type="text"
            value={displayName}
            onChange={e => setDisplayName(e.target.value)}
            placeholder="Enter your name..."
            onKeyDown={e => e.key === 'Enter' && handleSetName()}
            maxLength={50}
          />
          <button className="primary" onClick={handleSetName}>
            Join Chat
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>ChatApp</h2>
        </div>

        {/* Pending invitations */}
        {pendingInvitations.length > 0 && (
          <div className="invitations-panel">
            <h4>📩 Invitations ({pendingInvitations.length})</h4>
            {pendingInvitations.map(inv => {
              const room = rooms?.find(r => r.id === inv.roomId);
              return (
                <div key={inv.id.toString()} className="invitation-item">
                  <span className="invitation-info">
                    {room?.name ?? 'Unknown room'}
                  </span>
                  <div className="invitation-actions">
                    <button
                      className="small primary"
                      onClick={() => handleRespondToInvitation(inv.id, true)}
                    >
                      ✓
                    </button>
                    <button
                      className="small danger"
                      onClick={() => handleRespondToInvitation(inv.id, false)}
                    >
                      ✗
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        <div className="room-list">
          {/* Public rooms */}
          <div className="room-section">
            <div className="room-section-title">Public Rooms</div>
            {publicRooms.map(room => {
              const isMember = myRoomIds.has(room.id);
              const unread = getUnreadCount(room.id);
              return (
                <div
                  key={room.id.toString()}
                  className={`room-item ${selectedRoomId === room.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoomId(room.id)}
                >
                  <div className="room-icon">#</div>
                  <span className="room-name">{room.name}</span>
                  {!isMember && (
                    <button
                      className="small"
                      onClick={e => {
                        e.stopPropagation();
                        handleJoinRoom(room.id);
                      }}
                    >
                      Join
                    </button>
                  )}
                  {unread > 0 && <span className="unread-badge">{unread}</span>}
                </div>
              );
            })}
          </div>

          {/* Private rooms */}
          {myPrivateRooms.length > 0 && (
            <div className="room-section">
              <div className="room-section-title">Private Rooms</div>
              {myPrivateRooms.map(room => {
                const unread = getUnreadCount(room.id);
                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item private ${selectedRoomId === room.id ? 'active' : ''}`}
                    onClick={() => setSelectedRoomId(room.id)}
                  >
                    <div className="room-icon">🔒</div>
                    <span className="room-name">{room.name}</span>
                    {unread > 0 && (
                      <span className="unread-badge">{unread}</span>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          {/* DMs */}
          {myDmRooms.length > 0 && (
            <div className="room-section">
              <div className="room-section-title">Direct Messages</div>
              {myDmRooms.map(room => {
                const unread = getUnreadCount(room.id);
                const partnerName = getDmPartnerName(room);
                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item dm ${selectedRoomId === room.id ? 'active' : ''}`}
                    onClick={() => setSelectedRoomId(room.id)}
                  >
                    <div className="room-icon">💬</div>
                    <span className="room-name">{partnerName}</span>
                    {unread > 0 && (
                      <span className="unread-badge">{unread}</span>
                    )}
                  </div>
                );
              })}
            </div>
          )}

          {/* Action buttons */}
          <div className="room-section">
            <button
              className="small"
              style={{ width: '100%' }}
              onClick={() => setShowCreateRoomModal(true)}
            >
              + Create Room
            </button>
            <button
              className="small secondary"
              style={{ width: '100%', marginTop: '8px' }}
              onClick={() => setShowDmModal(true)}
            >
              + New DM
            </button>
          </div>
        </div>

        {/* User section */}
        <div className="user-section">
          <div className="user-info">
            <div className="user-avatar">
              {getInitials(currentUser?.name)}
              <span
                className={`status-dot ${currentUser?.status ?? 'online'}`}
              />
            </div>
            <div className="user-details">
              <div className="name">{currentUser?.name}</div>
              <select
                value={currentUser?.status ?? 'online'}
                onChange={e => handleSetStatus(e.target.value)}
                style={{ fontSize: '0.75rem', padding: '2px 4px' }}
              >
                <option value="online">🟢 Online</option>
                <option value="away">🟡 Away</option>
                <option value="dnd">🔴 Do Not Disturb</option>
                <option value="invisible">⚫ Invisible</option>
              </select>
            </div>
          </div>
        </div>
      </div>

      {/* Main chat area */}
      <div className="chat-main">
        {selectedRoom ? (
          <>
            <div className="chat-header">
              <h2>
                <span className="hash">
                  {selectedRoom.isPrivate
                    ? selectedRoom.isDm
                      ? ''
                      : '🔒'
                    : '#'}
                </span>
                {selectedRoom.isDm
                  ? getDmPartnerName(selectedRoom)
                  : selectedRoom.name}
              </h2>
              <div className="header-actions">
                {isRoomAdmin &&
                  selectedRoom.isPrivate &&
                  !selectedRoom.isDm && (
                    <button
                      className="small"
                      onClick={() => setShowInviteModal(true)}
                    >
                      Invite
                    </button>
                  )}
                {isRoomMember && !selectedRoom.isDm && (
                  <button
                    className="small secondary"
                    onClick={() => setShowMembersModal(true)}
                  >
                    Members
                  </button>
                )}
                {isRoomMember && !selectedRoom.isDm && (
                  <button
                    className="small danger"
                    onClick={() => handleLeaveRoom(selectedRoom.id)}
                  >
                    Leave
                  </button>
                )}
                {viewingThread && (
                  <button
                    className="small"
                    onClick={() => setViewingThread(null)}
                  >
                    ← Back to main
                  </button>
                )}
              </div>
            </div>

            {/* Scheduled messages panel */}
            {scheduledMessages.length > 0 && (
              <div className="scheduled-panel" style={{ margin: '8px 16px' }}>
                <h4>⏰ Scheduled Messages</h4>
                {scheduledMessages.map(s => (
                  <div
                    key={s.scheduledId.toString()}
                    className="scheduled-item"
                  >
                    <span className="content">{s.content}</span>
                    <span className="time">
                      {s.scheduledAt.tag === 'Time'
                        ? new Date(
                            Number(
                              s.scheduledAt.value.microsSinceUnixEpoch / 1000n
                            )
                          ).toLocaleString()
                        : 'pending'}
                    </span>
                    <button
                      className="small danger"
                      onClick={() => handleCancelScheduled(s.scheduledId)}
                    >
                      Cancel
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Messages */}
            <div className="messages-container" onScroll={handleScroll}>
              {!isRoomMember && !selectedRoom.isPrivate ? (
                <div className="empty-state">
                  <div className="icon">🚪</div>
                  <h3>Join to participate</h3>
                  <p>Click "Join" to start chatting in this room</p>
                  <button
                    className="primary"
                    style={{ marginTop: '16px' }}
                    onClick={() => handleJoinRoom(selectedRoom.id)}
                  >
                    Join Room
                  </button>
                </div>
              ) : !isRoomMember && selectedRoom.isPrivate ? (
                <div className="empty-state">
                  <div className="icon">🔒</div>
                  <h3>Private Room</h3>
                  <p>You need an invitation to join this room</p>
                </div>
              ) : topLevelMessages.length === 0 ? (
                <div className="empty-state">
                  <div className="icon">💬</div>
                  <h3>No messages yet</h3>
                  <p>Be the first to send a message!</p>
                </div>
              ) : (
                <div className="messages-list">
                  {viewingThread && (
                    <div className="thread-view-header">
                      <h4>Thread</h4>
                    </div>
                  )}
                  {topLevelMessages.map(msg => {
                    const msgReactions = getMessageReactions(msg.id);
                    const receipts = getReadReceipts(msg.id);
                    const replyCount = getReplyCount(msg.id);
                    const isMyMessage =
                      myIdentity &&
                      msg.senderIdentity.toHexString() ===
                        myIdentity.toHexString();
                    const isReply = msg.parentMessageId != null;

                    return (
                      <div
                        key={msg.id.toString()}
                        className={`message ${msg.isEphemeral ? 'ephemeral' : ''} ${isReply ? 'thread-reply' : ''}`}
                      >
                        <div className="message-avatar">
                          {getInitials(getUserName(msg.senderIdentity))}
                        </div>
                        <div className="message-content">
                          <div className="message-header">
                            <span className="message-author">
                              {getUserName(msg.senderIdentity)}
                            </span>
                            <span className="message-time">
                              {formatTime(msg.createdAt)}
                            </span>
                            {msg.isEdited && (
                              <span className="message-edited">(edited)</span>
                            )}
                          </div>

                          {editingMessage === msg.id ? (
                            <div
                              style={{
                                display: 'flex',
                                gap: '8px',
                                marginTop: '4px',
                              }}
                            >
                              <input
                                type="text"
                                value={editContent}
                                onChange={e => setEditContent(e.target.value)}
                                onKeyDown={e =>
                                  e.key === 'Enter' && handleSaveEdit()
                                }
                                style={{ flex: 1 }}
                              />
                              <button
                                className="small primary"
                                onClick={handleSaveEdit}
                              >
                                Save
                              </button>
                              <button
                                className="small"
                                onClick={() => setEditingMessage(null)}
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            <div className="message-body">{msg.content}</div>
                          )}

                          {msg.isEphemeral && msg.expiresAt && (
                            <div className="message-ephemeral-indicator">
                              ⏱️ Disappears in {formatCountdown(msg.expiresAt)}
                            </div>
                          )}

                          {/* Reactions */}
                          {msgReactions.size > 0 && (
                            <div className="reactions">
                              {Array.from(msgReactions.entries()).map(
                                ([emoji, data]) => (
                                  <div
                                    key={emoji}
                                    className={`reaction ${data.userReacted ? 'user-reacted' : ''}`}
                                    onClick={() =>
                                      handleToggleReaction(msg.id, emoji)
                                    }
                                    data-tooltip={data.users.join(', ')}
                                  >
                                    {emoji}{' '}
                                    <span className="reaction-count">
                                      {data.count}
                                    </span>
                                  </div>
                                )
                              )}
                            </div>
                          )}

                          {/* Thread indicator */}
                          {replyCount > 0 && !viewingThread && (
                            <div
                              className="thread-indicator"
                              onClick={() => setViewingThread(msg.id)}
                            >
                              💬 {replyCount}{' '}
                              {replyCount === 1 ? 'reply' : 'replies'}
                            </div>
                          )}

                          {/* Read receipts */}
                          {receipts.length > 0 && (
                            <div className="read-receipts">
                              <div className="avatars">
                                {receipts.slice(0, 5).map((name, i) => (
                                  <div key={i} className="mini-avatar">
                                    {getInitials(name)}
                                  </div>
                                ))}
                              </div>
                              <span>
                                Seen by{' '}
                                {receipts.length <= 3
                                  ? receipts.join(', ')
                                  : `${receipts.length} people`}
                              </span>
                            </div>
                          )}

                          {/* Message actions */}
                          <div className="message-actions">
                            <button
                              className="small icon-btn"
                              onClick={() =>
                                setShowReactionPicker(
                                  showReactionPicker === msg.id ? null : msg.id
                                )
                              }
                            >
                              😀
                            </button>
                            {!viewingThread && (
                              <button
                                className="small icon-btn"
                                onClick={() => setReplyingTo(msg.id)}
                              >
                                ↩️
                              </button>
                            )}
                            {isMyMessage && (
                              <>
                                <button
                                  className="small icon-btn"
                                  onClick={() =>
                                    handleEditMessage(msg.id, msg.content)
                                  }
                                >
                                  ✏️
                                </button>
                                <button
                                  className="small icon-btn"
                                  onClick={() => handleDeleteMessage(msg.id)}
                                >
                                  🗑️
                                </button>
                              </>
                            )}
                            {msg.isEdited && (
                              <button
                                className="small icon-btn"
                                onClick={() => setShowEditHistoryModal(msg.id)}
                              >
                                📜
                              </button>
                            )}
                          </div>

                          {/* Reaction picker */}
                          {showReactionPicker === msg.id && (
                            <div
                              className="reactions"
                              style={{
                                marginTop: '8px',
                                background: 'var(--bg-medium)',
                                padding: '8px',
                                borderRadius: '8px',
                              }}
                            >
                              {EMOJI_OPTIONS.map(emoji => (
                                <button
                                  key={emoji}
                                  className="small icon-btn"
                                  onClick={() =>
                                    handleToggleReaction(msg.id, emoji)
                                  }
                                >
                                  {emoji}
                                </button>
                              ))}
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                  <div ref={messagesEndRef} />
                </div>
              )}
            </div>

            {/* Typing indicator */}
            {typingIndicators.length > 0 && (
              <div className="typing-indicator">
                {typingIndicators.length === 1
                  ? `${getUserName(typingIndicators[0].userIdentity)} is typing...`
                  : `${typingIndicators.length} users are typing...`}
              </div>
            )}

            {/* Message input */}
            {isRoomMember && (
              <div className="message-input-container">
                {replyingTo && (
                  <div
                    style={{
                      marginBottom: '8px',
                      fontSize: '0.85rem',
                      color: 'var(--text-muted)',
                    }}
                  >
                    Replying to message...{' '}
                    <button
                      className="small"
                      onClick={() => setReplyingTo(null)}
                    >
                      Cancel
                    </button>
                  </div>
                )}
                <div className="message-input-wrapper">
                  <div className="message-input-main">
                    <div className="message-options">
                      <label className="checkbox-label">
                        <input
                          type="checkbox"
                          checked={isEphemeral}
                          onChange={e => setIsEphemeral(e.target.checked)}
                        />
                        Disappearing
                      </label>
                      {isEphemeral && (
                        <select
                          value={ephemeralDuration}
                          onChange={e =>
                            setEphemeralDuration(Number(e.target.value))
                          }
                        >
                          <option value={60}>1 min</option>
                          <option value={300}>5 min</option>
                          <option value={3600}>1 hour</option>
                        </select>
                      )}
                    </div>
                    <input
                      type="text"
                      className="message-input"
                      value={messageInput}
                      onChange={e => {
                        setMessageInput(e.target.value);
                        handleTyping();
                      }}
                      onKeyDown={e =>
                        e.key === 'Enter' && !e.shiftKey && handleSendMessage()
                      }
                      placeholder={
                        replyingTo ? 'Reply...' : 'Type a message...'
                      }
                    />
                  </div>
                  <button className="primary" onClick={handleSendMessage}>
                    Send
                  </button>
                  <button
                    className="secondary"
                    onClick={() => setShowScheduleModal(true)}
                  >
                    ⏰
                  </button>
                </div>
              </div>
            )}
          </>
        ) : (
          <div className="empty-state">
            <div className="icon">💬</div>
            <h3>Select a room</h3>
            <p>Choose a room from the sidebar to start chatting</p>
          </div>
        )}
      </div>

      {/* Online users panel */}
      <div className="online-panel">
        <h3>Online — {onlineUsers.length}</h3>
        <div className="online-list">
          {onlineUsers.map(user => (
            <div key={user.identity.toHexString()} className="online-user">
              <div className="avatar">
                {getInitials(user.name)}
                <span className={`status-dot ${user.status}`} />
              </div>
              <span className="user-name">{user.name}</span>
            </div>
          ))}
        </div>

        {/* Offline users */}
        <h3 style={{ marginTop: '16px' }}>Offline</h3>
        <div className="online-list">
          {users
            ?.filter(u => !u.online || u.status === 'invisible')
            .filter(u => u.name != null)
            .map(user => (
              <div key={user.identity.toHexString()} className="online-user">
                <div className="avatar">
                  {getInitials(user.name)}
                  <span className="status-dot invisible" />
                </div>
                <div style={{ display: 'flex', flexDirection: 'column' }}>
                  <span className="user-name">{user.name}</span>
                  <span className="last-active">
                    {formatRelativeTime(user.lastActive)}
                  </span>
                </div>
              </div>
            ))}
        </div>
      </div>

      {/* Modals */}
      {showCreateRoomModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowCreateRoomModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Create Room</h3>
            <input
              type="text"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              placeholder="Room name..."
              style={{ width: '100%', marginBottom: '12px' }}
            />
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={newRoomPrivate}
                onChange={e => setNewRoomPrivate(e.target.checked)}
              />
              Private room (invite-only)
            </label>
            <div className="modal-actions">
              <button onClick={() => setShowCreateRoomModal(false)}>
                Cancel
              </button>
              <button className="primary" onClick={handleCreateRoom}>
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {showInviteModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowInviteModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Invite User</h3>
            <input
              type="text"
              value={inviteUsername}
              onChange={e => setInviteUsername(e.target.value)}
              placeholder="Username..."
              style={{ width: '100%' }}
            />
            <div className="modal-actions">
              <button onClick={() => setShowInviteModal(false)}>Cancel</button>
              <button className="primary" onClick={handleInviteUser}>
                Invite
              </button>
            </div>
          </div>
        </div>
      )}

      {showDmModal && (
        <div className="modal-overlay" onClick={() => setShowDmModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Start Direct Message</h3>
            <input
              type="text"
              value={dmUsername}
              onChange={e => setDmUsername(e.target.value)}
              placeholder="Username..."
              style={{ width: '100%' }}
            />
            <div className="modal-actions">
              <button onClick={() => setShowDmModal(false)}>Cancel</button>
              <button className="primary" onClick={handleStartDm}>
                Start DM
              </button>
            </div>
          </div>
        </div>
      )}

      {showScheduleModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowScheduleModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Schedule Message</h3>
            <p
              style={{
                marginBottom: '12px',
                color: 'var(--text-muted)',
                fontSize: '0.9rem',
              }}
            >
              Message: "{messageInput}"
            </p>
            <input
              type="datetime-local"
              value={scheduleDateTime}
              onChange={e => setScheduleDateTime(e.target.value)}
              min={new Date().toISOString().slice(0, 16)}
              style={{ width: '100%' }}
            />
            <div className="modal-actions">
              <button onClick={() => setShowScheduleModal(false)}>
                Cancel
              </button>
              <button className="primary" onClick={handleScheduleMessage}>
                Schedule
              </button>
            </div>
          </div>
        </div>
      )}

      {showEditHistoryModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowEditHistoryModal(null)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            <div className="edit-history">
              {getEditHistory(showEditHistoryModal).map(edit => (
                <div key={edit.id.toString()} className="edit-entry">
                  <div className="edit-time">{formatTime(edit.editedAt)}</div>
                  <div className="edit-content">{edit.previousContent}</div>
                </div>
              ))}
            </div>
            <div className="modal-actions">
              <button onClick={() => setShowEditHistoryModal(null)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}

      {showMembersModal && selectedRoom && (
        <div
          className="modal-overlay"
          onClick={() => setShowMembersModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Room Members</h3>
            <div className="members-list">
              {currentRoomMembers.map(member => {
                const memberName = getUserName(member.userIdentity);
                const isMe =
                  myIdentity &&
                  member.userIdentity.toHexString() ===
                    myIdentity.toHexString();
                return (
                  <div key={member.id.toString()} className="member-item">
                    <div className="member-info">
                      <span>{memberName}</span>
                      {member.isAdmin && (
                        <span className="member-badge">Admin</span>
                      )}
                      {isMe && (
                        <span
                          className="member-badge"
                          style={{
                            background: 'var(--bg-light)',
                            color: 'var(--text-muted)',
                          }}
                        >
                          You
                        </span>
                      )}
                    </div>
                    {isRoomAdmin && !isMe && (
                      <div className="member-actions">
                        {!member.isAdmin && (
                          <button
                            className="small"
                            onClick={() => handlePromoteUser(memberName)}
                          >
                            Promote
                          </button>
                        )}
                        <button
                          className="small"
                          onClick={() => handleKickUser(memberName)}
                        >
                          Kick
                        </button>
                        <button
                          className="small danger"
                          onClick={() => handleBanUser(memberName)}
                        >
                          Ban
                        </button>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
            <div className="modal-actions">
              <button onClick={() => setShowMembersModal(false)}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
