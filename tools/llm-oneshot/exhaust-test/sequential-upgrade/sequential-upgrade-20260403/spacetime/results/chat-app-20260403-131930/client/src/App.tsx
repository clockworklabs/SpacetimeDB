import { useEffect, useRef, useState } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Room, Message, User, TypingIndicator, ReadReceipt, ScheduledMessage, MessageReaction, MessageEdit, RoomMember, ThreadReply, RoomInvitation, MessageDraft } from './module_bindings/types';

// ---- Timestamp helper ----
function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  const ms = Number(ts.microsSinceUnixEpoch / 1000n);
  const d = new Date(ms);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ---- Status helpers ----
type UserStatus = 'online' | 'away' | 'dnd' | 'invisible' | 'offline';

function getStatusColor(status: UserStatus): string {
  switch (status) {
    case 'online': return 'var(--primary)';      // green
    case 'away': return 'var(--warning)';         // yellow
    case 'dnd': return 'var(--danger)';           // red
    case 'invisible': return 'var(--text-muted)'; // grey
    case 'offline': return 'var(--text-muted)';   // grey
  }
}

function getEffectiveStatus(user: User): UserStatus {
  if (!user.online && user.status !== 'invisible') return 'offline';
  return (user.status as UserStatus) || 'offline';
}

function formatLastActive(ts: { microsSinceUnixEpoch: bigint }): string {
  const ms = Number(ts.microsSinceUnixEpoch / 1000n);
  if (ms === 0) return '';
  const diff = Date.now() - ms;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'Last active just now';
  if (mins < 60) return `Last active ${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `Last active ${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `Last active ${days}d ago`;
}

function StatusDot({ status }: { status: UserStatus }) {
  return (
    <span
      className="status-dot"
      style={{ background: getStatusColor(status) }}
      title={status}
    />
  );
}

// ---- Main App ----
export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [activeRoomId, setActiveRoomId] = useState<bigint | null>(null);
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [kickedFromRoom, setKickedFromRoom] = useState<string | null>(null);
  const [, setActivityTick] = useState(0);
  const pendingDmNameRef = useRef<string | null>(null);
  const confirmedMemberRef = useRef(false);
  const prevActiveRoomIdRef = useRef<bigint | null>(null);

  // Tick every 30s to keep activity badges fresh
  useEffect(() => {
    const timer = setInterval(() => setActivityTick(t => t + 1), 30_000);
    return () => clearInterval(timer);
  }, []);

  // Auto-away: set status to 'away' after 5 minutes of inactivity
  const autoAwayTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const currentStatusRef = useRef<string>('online');
  useEffect(() => {
    if (!isActive || !conn) return;
    const AUTO_AWAY_MS = 5 * 60 * 1000;
    const resetTimer = () => {
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
      // If currently away due to auto-away, restore to online on activity
      if (currentStatusRef.current === 'away') {
        conn?.reducers.setStatus({ status: 'online' });
        currentStatusRef.current = 'online';
      }
      autoAwayTimerRef.current = setTimeout(() => {
        if (currentStatusRef.current === 'online') {
          conn?.reducers.setStatus({ status: 'away' });
          currentStatusRef.current = 'away';
        }
      }, AUTO_AWAY_MS);
    };
    const events = ['mousemove', 'keydown', 'click', 'scroll'];
    events.forEach(e => window.addEventListener(e, resetTimer, { passive: true }));
    resetTimer();
    return () => {
      events.forEach(e => window.removeEventListener(e, resetTimer));
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
    };
  }, [isActive, conn]);

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe when connected
  useEffect(() => {
    if (!conn || !isActive) return;
    const sub = conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM scheduled_message',
        'SELECT * FROM message_reaction',
        'SELECT * FROM message_edit',
        'SELECT * FROM thread_reply',
        'SELECT * FROM room_invitation',
        'SELECT * FROM message_draft',
      ]);
    return () => { sub.unsubscribe(); };
  }, [conn, isActive]);

  // Reactive data
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [members] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [messageReactions] = useTable(tables.messageReaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [threadReplies] = useTable(tables.threadReply);
  const [roomInvitations] = useTable(tables.roomInvitation);
  const [messageDrafts] = useTable(tables.messageDraft);

  const myHex = myIdentity?.toHexString();

  // Auto-navigate to newly created DM room
  useEffect(() => {
    if (!pendingDmNameRef.current) return;
    const dmRoom = rooms.find((r: Room) => r.name === pendingDmNameRef.current);
    if (dmRoom) {
      setActiveRoomId(dmRoom.id);
      pendingDmNameRef.current = null;
    }
  }, [rooms]);

  // Detect when kicked from active room
  useEffect(() => {
    // Reset confirmation when the active room changes
    if (activeRoomId !== prevActiveRoomIdRef.current) {
      confirmedMemberRef.current = false;
      prevActiveRoomIdRef.current = activeRoomId;
    }
    if (!activeRoomId || !myHex || !isActive) return;
    const isMember = members.some(
      (m) => m.userIdentity.toHexString() === myHex && m.roomId === activeRoomId
    );
    if (isMember) {
      confirmedMemberRef.current = true;
    } else if (confirmedMemberRef.current) {
      // Only show "kicked" if user was previously confirmed as a member
      const kickedRoom = rooms.find((r: Room) => r.id === activeRoomId);
      setKickedFromRoom(kickedRoom?.name ?? 'the room');
      setActiveRoomId(null);
    }
  }, [members, activeRoomId, myHex, isActive]);

  const me = myHex ? users.find((u: User) => u.identity.toHexString() === myHex) : null;
  const isRegistered = !!me;

  // Keep currentStatusRef in sync with actual user status
  useEffect(() => {
    if (me?.status) currentStatusRef.current = me.status;
  }, [me?.status]);

  // --- Loading state ---
  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        Connecting to SpacetimeDB…
      </div>
    );
  }

  // --- Register ---
  if (!isRegistered) {
    return (
      <RegisterScreen
        conn={conn}
      />
    );
  }

  // Compute my rooms
  const myMemberships = members.filter((m) => m.userIdentity.toHexString() === myHex);
  const myRoomIds = new Set(myMemberships.map((m) => m.roomId));
  const myRooms = rooms.filter((r: Room) => myRoomIds.has(r.id) && !r.isPrivate && !r.isDm);
  const myPrivateRooms = rooms.filter((r: Room) => myRoomIds.has(r.id) && r.isPrivate && !r.isDm);
  const myDmRooms = rooms.filter((r: Room) => myRoomIds.has(r.id) && r.isDm);
  // Only show public rooms in the "Other Rooms" list
  const otherRooms = rooms.filter((r: Room) => !myRoomIds.has(r.id) && !r.isPrivate && !r.isDm);

  // My pending invitations
  const myInvitations = myHex
    ? roomInvitations.filter((inv: RoomInvitation) => inv.invitedUser.toHexString() === myHex)
    : [];

  // Helper: get the other user's name in a DM room
  const getDmPartnerName = (dmRoom: Room): string => {
    const dmMembers = members.filter((m) => m.roomId === dmRoom.id);
    const other = dmMembers.find((m) => m.userIdentity.toHexString() !== myHex);
    if (!other) return 'Unknown';
    const otherUser = users.find((u: User) => u.identity.toHexString() === other.userIdentity.toHexString());
    return otherUser?.name ?? 'Unknown';
  };

  const activeRoom = activeRoomId ? rooms.find((r: Room) => r.id === activeRoomId) : null;
  const roomMessages = activeRoomId
    ? messages.filter((m: Message) => m.roomId === activeRoomId)
        .sort((a: Message, b: Message) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    : [];

  // onlineUsers kept for reference but display now shows all users with status
  const _onlineUsers = users.filter((u: User) => u.online); void _onlineUsers;

  // Unread counts per room
  const unreadCounts: Map<bigint, number> = new Map();
  for (const room of myRooms) {
    const receipt = readReceipts.find(
      (r: ReadReceipt) => r.roomId === room.id && r.userIdentity.toHexString() === myHex
    );
    const lastReadId = receipt ? receipt.lastReadMessageId : 0n;
    const count = messages.filter(
      (m: Message) => m.roomId === room.id && m.id > lastReadId && m.sender.toHexString() !== myHex
    ).length;
    unreadCounts.set(room.id, count);
  }

  // Room activity: 'hot' = 5+ msgs in 2 min, 'active' = 1+ msgs in 5 min
  const getRoomActivity = (roomId: bigint): 'hot' | 'active' | null => {
    const nowMicros = BigInt(Date.now()) * 1000n;
    const twoMinMicros = 2n * 60n * 1_000_000n;
    const fiveMinMicros = 5n * 60n * 1_000_000n;
    const roomMsgs = messages.filter((m: Message) => m.roomId === roomId);
    const recentFive = roomMsgs.filter((m: Message) => nowMicros - m.sentAt.microsSinceUnixEpoch <= fiveMinMicros);
    if (recentFive.length === 0) return null;
    const recentTwo = recentFive.filter((m: Message) => nowMicros - m.sentAt.microsSinceUnixEpoch <= twoMinMicros);
    if (recentTwo.length >= 5) return 'hot';
    return 'active';
  };

  // Active room members
  const activeRoomMembers = activeRoomId
    ? members.filter((m) => m.roomId === activeRoomId)
    : [];
  const amIAdminInActiveRoom = activeRoomMembers.some(
    (m) => m.userIdentity.toHexString() === myHex && m.isAdmin
  );

  // My scheduled messages for the active room
  const myRoomScheduled = activeRoomId
    ? scheduledMessages.filter(
        (sm: ScheduledMessage) => sm.roomId === activeRoomId && sm.sender.toHexString() === myHex
      )
    : [];

  // Reactions for the active room
  const roomReactions = activeRoomId
    ? messageReactions.filter((r: MessageReaction) => r.roomId === activeRoomId)
    : [];

  // Edit history for messages in the active room
  const roomMessageIds = new Set(roomMessages.map((m: Message) => m.id));
  const roomMessageEdits = activeRoomId
    ? messageEdits.filter((e: MessageEdit) => roomMessageIds.has(e.messageId))
    : [];

  // Thread replies for the active room
  const roomThreadReplies = activeRoomId
    ? threadReplies.filter((r: ThreadReply) => r.roomId === activeRoomId)
    : [];

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="sidebar-title">SpacetimeDB Chat</div>
        </div>
        <div className="sidebar-user">
          <StatusDot status={getEffectiveStatus(me)} />
          <span style={{ fontWeight: 600, flex: 1 }}>{me.name}</span>
          <select
            value={me.status || 'online'}
            onChange={(e) => {
              const s = e.target.value;
              currentStatusRef.current = s;
              conn?.reducers.setStatus({ status: s });
            }}
            style={{ fontSize: 11, background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: 4, padding: '2px 4px', cursor: 'pointer' }}
            title="Set your status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>
        <div className="sidebar-section">
          {/* Pending Invitations */}
          {myInvitations.length > 0 && (
            <>
              <div className="sidebar-section-header" style={{ color: 'var(--warning)' }}>
                <span>Invitations ({myInvitations.length})</span>
              </div>
              {myInvitations.map((inv: RoomInvitation) => {
                const invRoom = rooms.find((r: Room) => r.id === inv.roomId);
                const inviter = users.find((u: User) => u.identity.toHexString() === inv.invitedBy.toHexString());
                return (
                  <div key={String(inv.id)} style={{ padding: '6px 12px', background: 'var(--surface)', margin: '2px 8px', borderRadius: 6, border: '1px solid var(--warning)', fontSize: 12 }}>
                    <div style={{ color: 'var(--text)', marginBottom: 4 }}>
                      <strong>{inviter?.name ?? 'Someone'}</strong> invited you to <strong>#{invRoom?.name ?? 'a room'}</strong>
                    </div>
                    <div style={{ display: 'flex', gap: 6 }}>
                      <button className="primary" style={{ fontSize: 11, padding: '2px 8px' }} onClick={() => {
                        conn?.reducers.acceptInvitation({ invitationId: inv.id });
                        setActiveRoomId(inv.roomId);
                      }}>Accept</button>
                      <button className="danger" style={{ fontSize: 11, padding: '2px 8px' }} onClick={() =>
                        conn?.reducers.declineInvitation({ invitationId: inv.id })
                      }>Decline</button>
                    </div>
                  </div>
                );
              })}
            </>
          )}

          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
          </div>
          {myRooms.length === 0 && otherRooms.length === 0 && myPrivateRooms.length === 0 && myDmRooms.length === 0 && (
            <div style={{ padding: '8px 16px', color: 'var(--text-muted)', fontSize: 12 }}>
              No rooms yet. Create one!
            </div>
          )}
          {myRooms.map((r: Room) => {
            const activity = getRoomActivity(r.id);
            return (
              <div
                key={String(r.id)}
                className={`room-item ${activeRoomId === r.id ? 'active' : ''}`}
                onClick={() => setActiveRoomId(r.id)}
              >
                <span className="room-name"># {r.name}</span>
                {activity === 'hot' && <span className="activity-badge hot" title="Very active">🔥 Hot</span>}
                {activity === 'active' && <span className="activity-badge active" title="Recently active">Active</span>}
                {(unreadCounts.get(r.id) ?? 0) > 0 && (
                  <span className="unread-badge">{unreadCounts.get(r.id)}</span>
                )}
              </div>
            );
          })}
          {otherRooms.length > 0 && (
            <>
              <div className="sidebar-section-header" style={{ marginTop: 8 }}>
                <span>Other Rooms</span>
              </div>
              {otherRooms.map((r: Room) => {
                const activity = getRoomActivity(r.id);
                return (
                  <div
                    key={String(r.id)}
                    className="room-item"
                    onClick={() => {
                      conn?.reducers.joinRoom({ roomId: r.id });
                      setActiveRoomId(r.id);
                    }}
                  >
                    <span className="room-name"># {r.name}</span>
                    {activity === 'hot' && <span className="activity-badge hot" title="Very active">🔥 Hot</span>}
                    {activity === 'active' && <span className="activity-badge active" title="Recently active">Active</span>}
                    <button className="secondary" style={{ fontSize: 11, padding: '2px 8px' }}
                      onClick={(e) => {
                        e.stopPropagation();
                        conn?.reducers.joinRoom({ roomId: r.id });
                        setActiveRoomId(r.id);
                      }}>Join</button>
                  </div>
                );
              })}
            </>
          )}

          {/* Private Rooms */}
          {myPrivateRooms.length > 0 && (
            <>
              <div className="sidebar-section-header" style={{ marginTop: 8 }}>
                <span>Private Rooms</span>
              </div>
              {myPrivateRooms.map((r: Room) => {
                const activity = getRoomActivity(r.id);
                return (
                  <div
                    key={String(r.id)}
                    className={`room-item ${activeRoomId === r.id ? 'active' : ''}`}
                    onClick={() => setActiveRoomId(r.id)}
                  >
                    <span className="room-name">🔒 {r.name}</span>
                    {activity === 'hot' && <span className="activity-badge hot" title="Very active">🔥 Hot</span>}
                    {activity === 'active' && <span className="activity-badge active" title="Recently active">Active</span>}
                    {(unreadCounts.get(r.id) ?? 0) > 0 && (
                      <span className="unread-badge">{unreadCounts.get(r.id)}</span>
                    )}
                  </div>
                );
              })}
            </>
          )}

          {/* Direct Messages */}
          {myDmRooms.length > 0 && (
            <>
              <div className="sidebar-section-header" style={{ marginTop: 8 }}>
                <span>Direct Messages</span>
              </div>
              {myDmRooms.map((r: Room) => {
                const partnerName = getDmPartnerName(r);
                const activity = getRoomActivity(r.id);
                return (
                  <div
                    key={String(r.id)}
                    className={`room-item ${activeRoomId === r.id ? 'active' : ''}`}
                    onClick={() => setActiveRoomId(r.id)}
                  >
                    <span className="room-name">@ {partnerName}</span>
                    {activity === 'hot' && <span className="activity-badge hot" title="Very active">🔥 Hot</span>}
                    {activity === 'active' && <span className="activity-badge active" title="Recently active">Active</span>}
                    {(unreadCounts.get(r.id) ?? 0) > 0 && (
                      <span className="unread-badge">{unreadCounts.get(r.id)}</span>
                    )}
                  </div>
                );
              })}
            </>
          )}

          <div className="sidebar-section-header" style={{ marginTop: 8 }}>
            <span>Users ({users.length})</span>
          </div>
          {users.map((u: User) => {
            const effectiveStatus = getEffectiveStatus(u);
            const isMe = u.identity.toHexString() === myHex;
            const lastActive = (!u.online && u.status !== 'invisible') ? formatLastActive(u.lastActiveAt) : '';
            return (
              <div key={u.identity.toHexString()} className="online-user-item" title={lastActive || effectiveStatus}>
                <StatusDot status={effectiveStatus} />
                <span style={{ flex: 1 }}>{u.name}{isMe ? ' (you)' : ''}</span>
                {!isMe && (
                  <button
                    className="secondary"
                    style={{ fontSize: 10, padding: '1px 6px', marginLeft: 4 }}
                    title={`Send DM to ${u.name}`}
                    onClick={() => {
                      // Check if DM already exists
                      const myHexStr = myIdentity?.toHexString() ?? '';
                      const targetHex = u.identity.toHexString();
                      const sortedHexes = [myHexStr, targetHex].sort();
                      const dmName = `dm-${sortedHexes[0].slice(0, 8)}-${sortedHexes[1].slice(0, 8)}`;
                      const existingDm = rooms.find((r: Room) => r.name === dmName);
                      if (existingDm) {
                        setActiveRoomId(existingDm.id);
                      } else {
                        pendingDmNameRef.current = dmName;
                        conn?.reducers.createDm({ targetName: u.name });
                      }
                    }}
                  >DM</button>
                )}
                {lastActive && <span style={{ fontSize: 10, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{lastActive}</span>}
              </div>
            );
          })}
        </div>
      </aside>

      {/* Main */}
      <main className="main">
        {kickedFromRoom && (
          <div className="kicked-notification" onClick={() => setKickedFromRoom(null)}>
            <span>You have been kicked from <strong>#{kickedFromRoom}</strong>. Click to dismiss.</span>
          </div>
        )}
        {activeRoom ? (
          <RoomView
            room={activeRoom}
            messages={roomMessages}
            users={users}
            myHex={myHex!}
            typingIndicators={typingIndicators}
            readReceipts={readReceipts}
            scheduledMessages={myRoomScheduled}
            reactions={roomReactions}
            messageEdits={roomMessageEdits}
            threadReplies={roomThreadReplies}
            roomMembers={activeRoomMembers}
            amIAdmin={amIAdminInActiveRoom}
            conn={conn}
            draftText={messageDrafts.find((d: MessageDraft) => d.roomId === activeRoom.id && d.userIdentity.toHexString() === myHex)?.text ?? ''}
            onLeave={() => {
              conn?.reducers.leaveRoom({ roomId: activeRoom.id });
              setActiveRoomId(null);
            }}
          />
        ) : (
          <div className="empty-state">
            <h3>Welcome, {me.name}!</h3>
            <p>Select a room from the sidebar to start chatting,<br/>or create a new room.</p>
            <button className="primary" onClick={() => setShowCreateRoom(true)}>+ Create Room</button>
          </div>
        )}
      </main>

      {showCreateRoom && (
        <CreateRoomModal
          conn={conn}
          onClose={() => setShowCreateRoom(false)}
          rooms={rooms}
        />
      )}

    </div>
  );
}

// ---- Register Screen ----
function RegisterScreen({ conn }: { conn: DbConnection | null }) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  const handleSubmit = () => {
    const trimmed = name.trim();
    if (!trimmed) { setError('Please enter a name'); return; }
    if (trimmed.length > 32) { setError('Name too long (max 32)'); return; }
    setError('');
    conn?.reducers.register({ name: trimmed });
  };

  return (
    <div className="login-screen">
      <div className="login-card">
        <div className="login-title">SpacetimeDB Chat</div>
        <div className="login-subtitle">Enter a display name to get started</div>
        <div className="form-group">
          <input
            type="text"
            placeholder="Enter your name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleSubmit()}
            autoFocus
            maxLength={32}
          />
          {error && <div className="error-msg">{error}</div>}
        </div>
        <button className="primary" onClick={handleSubmit} type="submit">Join Chat</button>
      </div>
    </div>
  );
}

// ---- Create Room Modal ----
function CreateRoomModal({
  conn, onClose, rooms
}: {
  conn: DbConnection | null;
  onClose: () => void;
  rooms: ReadonlyArray<Room>;
}) {
  const [name, setName] = useState('');
  const [isPrivate, setIsPrivate] = useState(false);
  const [error, setError] = useState('');

  const handleCreate = () => {
    const trimmed = name.trim();
    if (!trimmed) { setError('Room name required'); return; }
    if (trimmed.length > 64) { setError('Name too long (max 64)'); return; }
    const exists = rooms.some((r: Room) => r.name.toLowerCase() === trimmed.toLowerCase());
    if (exists) { setError('Room already exists'); return; }
    setError('');
    conn?.reducers.createRoom({ name: trimmed, isPrivate });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Create a New Room</div>
        <div className="form-group">
          <input
            type="text"
            placeholder="Room name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleCreate(); if (e.key === 'Escape') onClose(); }}
            autoFocus
            maxLength={64}
          />
          {error && <div className="error-msg">{error}</div>}
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0' }}>
          <input
            type="checkbox"
            id="private-room-checkbox"
            checked={isPrivate}
            onChange={(e) => setIsPrivate(e.target.checked)}
            style={{ cursor: 'pointer' }}
          />
          <label htmlFor="private-room-checkbox" style={{ cursor: 'pointer', fontSize: 13, color: 'var(--text)' }}>
            🔒 Private room (invite-only, not shown in public list)
          </label>
        </div>
        <div className="modal-actions">
          <button className="secondary" onClick={onClose}>Cancel</button>
          <button className="primary" onClick={handleCreate}>Create Room</button>
        </div>
      </div>
    </div>
  );
}

// ---- Schedule Message Modal ----
function ScheduleMessageModal({
  conn, roomId, onClose
}: {
  conn: DbConnection | null;
  roomId: bigint;
  onClose: () => void;
}) {
  const [text, setText] = useState('');
  const [dateValue, setDateValue] = useState('');
  const [timeValue, setTimeValue] = useState('');
  const [error, setError] = useState('');

  // Default to 5 minutes from now
  useEffect(() => {
    const now = new Date(Date.now() + 5 * 60 * 1000);
    const dateStr = now.toISOString().slice(0, 10);
    const hours = String(now.getHours()).padStart(2, '0');
    const mins = String(now.getMinutes()).padStart(2, '0');
    setDateValue(dateStr);
    setTimeValue(`${hours}:${mins}`);
  }, []);

  const handleSchedule = () => {
    const trimmed = text.trim();
    if (!trimmed) { setError('Message cannot be empty'); return; }
    if (!dateValue || !timeValue) { setError('Please set a date and time'); return; }
    const sendAt = new Date(`${dateValue}T${timeValue}:00`);
    if (isNaN(sendAt.getTime())) { setError('Invalid date/time'); return; }
    if (sendAt.getTime() <= Date.now()) { setError('Scheduled time must be in the future'); return; }
    const sendAtMicros = BigInt(sendAt.getTime()) * 1000n;
    setError('');
    conn?.reducers.scheduleMessage({ roomId, text: trimmed, sendAtMicros });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Schedule a Message</div>
        <div className="form-group">
          <textarea
            placeholder="Type your message..."
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={3}
            maxLength={2000}
            style={{ width: '100%', resize: 'vertical', background: 'var(--bg)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: 6, padding: '8px 12px', fontFamily: 'inherit', fontSize: 14 }}
            autoFocus
          />
        </div>
        <div className="form-group" style={{ display: 'flex', gap: 8 }}>
          <input
            type="date"
            value={dateValue}
            onChange={(e) => setDateValue(e.target.value)}
            style={{ flex: 1 }}
          />
          <input
            type="time"
            value={timeValue}
            onChange={(e) => setTimeValue(e.target.value)}
            style={{ flex: 1 }}
          />
        </div>
        {error && <div className="error-msg">{error}</div>}
        <div className="modal-actions">
          <button className="secondary" onClick={onClose}>Cancel</button>
          <button className="primary" onClick={handleSchedule}>Schedule</button>
        </div>
      </div>
    </div>
  );
}

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

// ---- Invite User Modal ----
function InviteUserModal({
  conn, roomId, onClose
}: {
  conn: DbConnection | null;
  roomId: bigint;
  onClose: () => void;
}) {
  const [targetName, setTargetName] = useState('');
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');

  const handleInvite = () => {
    const trimmed = targetName.trim();
    if (!trimmed) { setError('Please enter a username'); return; }
    setError('');
    setSuccess('');
    conn?.reducers.inviteUser({ roomId, targetName: trimmed });
    setSuccess(`Invitation sent to ${trimmed}`);
    setTargetName('');
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Invite User to Private Room</div>
        <div className="form-group">
          <input
            type="text"
            placeholder="Enter username"
            value={targetName}
            onChange={(e) => setTargetName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleInvite(); if (e.key === 'Escape') onClose(); }}
            autoFocus
          />
          {error && <div className="error-msg">{error}</div>}
          {success && <div style={{ color: 'var(--primary)', fontSize: 13, marginTop: 4 }}>{success}</div>}
        </div>
        <div className="modal-actions">
          <button className="secondary" onClick={onClose}>Close</button>
          <button className="primary" onClick={handleInvite}>Send Invite</button>
        </div>
      </div>
    </div>
  );
}

// ---- Room View ----
function RoomView({
  room, messages, users, myHex, typingIndicators, readReceipts, scheduledMessages, reactions, messageEdits, threadReplies, roomMembers, amIAdmin, conn, draftText, onLeave
}: {
  room: Room;
  messages: ReadonlyArray<Message>;
  users: ReadonlyArray<User>;
  myHex: string;
  typingIndicators: ReadonlyArray<TypingIndicator>;
  readReceipts: ReadonlyArray<ReadReceipt>;
  scheduledMessages: ReadonlyArray<ScheduledMessage>;
  reactions: ReadonlyArray<MessageReaction>;
  messageEdits: ReadonlyArray<MessageEdit>;
  threadReplies: ReadonlyArray<ThreadReply>;
  roomMembers: ReadonlyArray<RoomMember>;
  amIAdmin: boolean;
  conn: DbConnection | null;
  draftText: string;
  onLeave: () => void;
}) {
  const [text, setText] = useState(draftText);
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const prevRoomIdRef = useRef<bigint>(room.id);
  const currentTextRef = useRef(draftText);
  const initialDraftAppliedRef = useRef(!!draftText);

  // Keep currentTextRef in sync with text state for use in cleanup
  useEffect(() => { currentTextRef.current = text; }, [text]);

  // Apply draft when it first arrives — subscription may load after component mounts
  useEffect(() => {
    if (!initialDraftAppliedRef.current && draftText) {
      initialDraftAppliedRef.current = true;
      setText(draftText);
    }
  }, [draftText]);

  // When switching to a new room, load that room's draft
  useEffect(() => {
    if (room.id !== prevRoomIdRef.current) {
      prevRoomIdRef.current = room.id;
      initialDraftAppliedRef.current = !!draftText;
      setText(draftText);
    }
  // Only run when roomId changes
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [room.id]);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [, setTick] = useState(0);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<bigint | null>(null);
  const [showMembersPanel, setShowMembersPanel] = useState(false);
  const [threadMessageId, setThreadMessageId] = useState<bigint | null>(null);
  const [showInviteModal, setShowInviteModal] = useState(false);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (isAtBottom && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isAtBottom]);

  // Mark messages as read when room is active
  useEffect(() => {
    if (messages.length === 0) return;
    const lastMsg = messages[messages.length - 1];
    conn?.reducers.markRead({ roomId: room.id, lastReadMessageId: lastMsg.id });
  }, [messages, room.id, conn]);

  const handleScroll = () => {
    const container = messagesContainerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    setIsAtBottom(scrollHeight - scrollTop - clientHeight < 50);
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    setIsAtBottom(true);
  };

  const handleTyping = () => {
    if (!isTypingRef.current) {
      isTypingRef.current = true;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: true });
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      isTypingRef.current = false;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
    }, 4000);
  };

  // Cleanup typing and draft save timer on unmount / room change
  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      if (isTypingRef.current) {
        conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
        isTypingRef.current = false;
      }
      if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
      // Always save current draft immediately when leaving a room
      conn?.reducers.saveDraft({ roomId: room.id, text: currentTextRef.current });
    };
  }, [room.id, conn]);

  // Tick every second to update ephemeral countdown displays
  useEffect(() => {
    const hasEphemeral = messages.some((m: Message) => m.expiresAtMicros > 0n);
    if (!hasEphemeral) return;
    const timer = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(timer);
  }, [messages]);

  const handleSend = () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText('');
    // Clear draft
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    conn?.reducers.saveDraft({ roomId: room.id, text: '' });
    // Clear typing
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    if (isTypingRef.current) {
      isTypingRef.current = false;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
    }
    if (isEphemeral) {
      conn?.reducers.sendEphemeralMessage({ roomId: room.id, text: trimmed, durationSeconds: ephemeralDuration });
    } else {
      conn?.reducers.sendMessage({ roomId: room.id, text: trimmed });
    }
  };

  const handleDraftChange = (newText: string) => {
    setText(newText);
    handleTyping();
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    draftSaveTimerRef.current = setTimeout(() => {
      conn?.reducers.saveDraft({ roomId: room.id, text: newText });
    }, 800);
  };

  // Format countdown for ephemeral messages
  const formatCountdown = (expiresAtMicros: bigint): string => {
    const nowMicros = BigInt(Date.now()) * 1000n;
    const remaining = expiresAtMicros - nowMicros;
    if (remaining <= 0n) return 'Expiring…';
    const secs = Number(remaining / 1_000_000n);
    if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
    if (secs >= 60) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    return `${secs}s`;
  };

  // Typing indicator text (other users typing in this room)
  const roomTyping = typingIndicators.filter(
    (ti: TypingIndicator) => ti.roomId === room.id && ti.userIdentity.toHexString() !== myHex
  );
  const typingNames = roomTyping.map((ti: TypingIndicator) => {
    const u = users.find((u: User) => u.identity.toHexString() === ti.userIdentity.toHexString());
    return u?.name ?? 'Someone';
  });

  let typingText = '';
  if (typingNames.length === 1) typingText = `${typingNames[0]} is typing...`;
  else if (typingNames.length === 2) typingText = `${typingNames[0]} and ${typingNames[1]} are typing...`;
  else if (typingNames.length > 2) typingText = 'Multiple users are typing...';

  // Group messages by sender
  const groupedMessages: { sender: string; senderName: string; isMe: boolean; items: Message[] }[] = [];
  for (const msg of messages) {
    const senderHex = msg.sender.toHexString();
    const isMe = senderHex === myHex;
    const senderUser = users.find((u: User) => u.identity.toHexString() === senderHex);
    const senderName = senderUser?.name ?? 'Unknown';
    const last = groupedMessages[groupedMessages.length - 1];
    if (last && last.sender === senderHex) {
      last.items.push(msg);
    } else {
      groupedMessages.push({ sender: senderHex, senderName, isMe, items: [msg] });
    }
  }

  // Read receipts for a message: users who have read up to this message ID (excluding the message sender)
  const getReadBy = (msgId: bigint, senderHex: string): string[] => {
    return readReceipts
      .filter((r: ReadReceipt) => r.roomId === room.id && r.lastReadMessageId >= msgId && r.userIdentity.toHexString() !== myHex && r.userIdentity.toHexString() !== senderHex)
      .map((r: ReadReceipt) => {
        const u = users.find((u: User) => u.identity.toHexString() === r.userIdentity.toHexString());
        return u?.name ?? 'Someone';
      });
  };

  // Format scheduled time from ScheduleAt
  const formatScheduledAt = (sm: ScheduledMessage): string => {
    const sat = sm.scheduledAt as any;
    let micros: bigint | null = null;
    if (sat && sat.tag === 'Time' && sat.value) {
      micros = BigInt(sat.value.__timestamp_micros_since_unix_epoch__ ?? 0n);
    } else if (typeof sat?.__timestamp_micros_since_unix_epoch__ === 'bigint') {
      micros = sat.__timestamp_micros_since_unix_epoch__;
    }
    if (micros === null) return 'unknown time';
    const ms = Number(micros / 1000n);
    return new Date(ms).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  };

  return (
    <>
      <div className="room-header">
        <span className="room-header-name">
          {room.isDm ? '@ ' : room.isPrivate ? '🔒 ' : '# '}{room.name}
          {room.isPrivate && !room.isDm && <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 6 }}>Private</span>}
        </span>
        <div style={{ display: 'flex', gap: 6 }}>
          {room.isPrivate && !room.isDm && amIAdmin && (
            <button className="secondary" onClick={() => setShowInviteModal(true)} style={{ fontSize: 12, padding: '4px 10px' }}>+ Invite</button>
          )}
          <button className="secondary" onClick={() => setShowMembersPanel(!showMembersPanel)} style={{ fontSize: 12, padding: '4px 10px' }}>Members</button>
          <button className="danger" onClick={onLeave} style={{ fontSize: 12, padding: '4px 10px' }}>Leave</button>
        </div>
      </div>

      {showMembersPanel && (
        <div className="members-panel">
          <div className="members-panel-title">Members ({roomMembers.length})</div>
          {roomMembers.map((m: RoomMember) => {
            const memberUser = users.find((u: User) => u.identity.toHexString() === m.userIdentity.toHexString());
            const isMe = m.userIdentity.toHexString() === myHex;
            const memberStatus = memberUser ? getEffectiveStatus(memberUser) : 'offline';
            return (
              <div key={String(m.id)} className="member-item">
                <StatusDot status={memberStatus} />
                <span className="member-name">{memberUser?.name ?? 'Unknown'}{isMe ? ' (you)' : ''}</span>
                {m.isAdmin && <span className="admin-badge">Admin</span>}
                {amIAdmin && !isMe && (
                  <div className="member-actions">
                    {!m.isAdmin && (
                      <button
                        className="primary"
                        style={{ fontSize: 11, padding: '2px 8px' }}
                        onClick={() => conn?.reducers.promoteUser({ roomId: room.id, targetIdentityHex: m.userIdentity.toHexString() })}
                      >Promote</button>
                    )}
                    <button
                      className="danger"
                      style={{ fontSize: 11, padding: '2px 8px' }}
                      onClick={() => conn?.reducers.kickUser({ roomId: room.id, targetIdentityHex: m.userIdentity.toHexString() })}
                    >Kick</button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Thread panel */}
      {threadMessageId !== null && (() => {
        const parentMsg = messages.find((m: Message) => m.id === threadMessageId);
        const replies = threadReplies
          .filter((r: ThreadReply) => r.parentMessageId === threadMessageId)
          .slice()
          .sort((a, b) => (a.id < b.id ? -1 : 1));
        return (
          <ThreadPanel
            parentMessage={parentMsg ?? null}
            replies={replies}
            users={users}
            myHex={myHex}
            roomId={room.id}
            conn={conn}
            onClose={() => setThreadMessageId(null)}
          />
        );
      })()}

      {/* Scheduled messages panel */}
      {scheduledMessages.length > 0 && (
        <div className="scheduled-panel">
          <div className="scheduled-panel-title">Scheduled Messages ({scheduledMessages.length})</div>
          {scheduledMessages.map((sm: ScheduledMessage) => (
            <div key={String(sm.scheduledId)} className="scheduled-item">
              <div className="scheduled-item-text">{sm.text}</div>
              <div className="scheduled-item-meta">
                <span className="scheduled-item-time">Sends at {formatScheduledAt(sm)}</span>
                <button
                  className="danger"
                  style={{ fontSize: 11, padding: '2px 8px' }}
                  onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: sm.scheduledId })}
                >
                  Cancel
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="messages-container">
        <div
          className="messages-area"
          ref={messagesContainerRef}
          onScroll={handleScroll}
        >
          {messages.length === 0 && (
            <div className="empty-state" style={{ flex: 1, paddingTop: 48 }}>
              <p>No messages yet. Say hello!</p>
            </div>
          )}
          {groupedMessages.map((group, gi) => (
            <div key={gi} className="message-group">
              <div className="message-header">
                <span className={`message-sender ${group.isMe ? 'me' : ''}`}>{group.senderName}</span>
                <span className="message-time">{formatTime(group.items[0].sentAt)}</span>
              </div>
              {group.items.map((msg: Message) => {
                const readBy = getReadBy(msg.id, group.sender);
                const isEphemeralMsg = msg.expiresAtMicros > 0n;
                const isEditing = editingMessageId === msg.id;
                const isShowingHistory = historyMessageId === msg.id;
                const msgEdits = messageEdits
                  .filter((e: MessageEdit) => e.messageId === msg.id)
                  .sort((a: MessageEdit, b: MessageEdit) => (a.editedAt.microsSinceUnixEpoch < b.editedAt.microsSinceUnixEpoch ? -1 : 1));
                const wasEdited = msgEdits.length > 0;
                // Group reactions by emoji for this message
                const msgReactions = reactions.filter((r: MessageReaction) => r.messageId === msg.id);
                const reactionGroups: { emoji: string; count: number; reactors: string[]; iMine: boolean }[] = [];
                for (const emoji of REACTION_EMOJIS) {
                  const emojiReactions = msgReactions.filter((r: MessageReaction) => r.emoji === emoji);
                  if (emojiReactions.length > 0) {
                    const reactors = emojiReactions.map((r: MessageReaction) => {
                      const u = users.find((u: User) => u.identity.toHexString() === r.userIdentity.toHexString());
                      return u?.name ?? 'Someone';
                    });
                    const iMine = emojiReactions.some((r: MessageReaction) => r.userIdentity.toHexString() === myHex);
                    reactionGroups.push({ emoji, count: emojiReactions.length, reactors, iMine });
                  }
                }
                const msgReplies = threadReplies.filter((r: ThreadReply) => r.parentMessageId === msg.id);
                const replyCount = msgReplies.length;
                const replyPreview = replyCount > 0
                  ? (() => {
                      const last = msgReplies.reduce((a, b) => a.id > b.id ? a : b);
                      const u = users.find((u: User) => u.identity.toHexString() === last.sender.toHexString());
                      return `${u?.name ?? 'Someone'}: ${last.text.slice(0, 40)}${last.text.length > 40 ? '…' : ''}`;
                    })()
                  : '';
                return (
                  <div key={String(msg.id)} className={`message-row${isEphemeralMsg ? ' ephemeral-message' : ''}`} style={{ position: 'relative' }}>
                    {isEditing ? (
                      <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                        <input
                          type="text"
                          value={editText}
                          onChange={(e) => setEditText(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter' && !e.shiftKey) {
                              e.preventDefault();
                              const trimmed = editText.trim();
                              if (trimmed) {
                                conn?.reducers.editMessage({ messageId: msg.id, newText: trimmed });
                              }
                              setEditingMessageId(null);
                            } else if (e.key === 'Escape') {
                              setEditingMessageId(null);
                            }
                          }}
                          autoFocus
                          maxLength={2000}
                          style={{ flex: 1, background: 'var(--bg)', color: 'var(--text)', border: '1px solid var(--accent)', borderRadius: 4, padding: '4px 8px', fontFamily: 'inherit', fontSize: 14 }}
                        />
                        <button className="primary" style={{ fontSize: 12, padding: '4px 10px' }} onClick={() => {
                          const trimmed = editText.trim();
                          if (trimmed) conn?.reducers.editMessage({ messageId: msg.id, newText: trimmed });
                          setEditingMessageId(null);
                        }}>Save</button>
                        <button className="secondary" style={{ fontSize: 12, padding: '4px 10px' }} onClick={() => setEditingMessageId(null)}>Cancel</button>
                      </div>
                    ) : (
                      <div style={{ display: 'flex', alignItems: 'baseline', gap: 6 }}>
                        <div className="message-text">{msg.text}</div>
                        {wasEdited && (
                          <button
                            onClick={() => setHistoryMessageId(isShowingHistory ? null : msg.id)}
                            style={{ fontSize: 11, color: 'var(--text-muted)', background: 'none', border: 'none', cursor: 'pointer', padding: 0, textDecoration: 'underline' }}
                            title="View edit history"
                          >(edited)</button>
                        )}
                        {group.isMe && !isEphemeralMsg && (
                          <button
                            className="edit-btn"
                            onClick={() => { setEditText(msg.text); setEditingMessageId(msg.id); setHistoryMessageId(null); }}
                            style={{ fontSize: 11, padding: '2px 8px', opacity: 0.7 }}
                          >Edit</button>
                        )}
                        <button
                          className="edit-btn"
                          onClick={() => setThreadMessageId(threadMessageId === msg.id ? null : msg.id)}
                          style={{ fontSize: 11, padding: '2px 8px', opacity: 0.7 }}
                          title="View thread"
                        >Reply</button>
                      </div>
                    )}
                    {replyCount > 0 && (
                      <button
                        onClick={() => setThreadMessageId(threadMessageId === msg.id ? null : msg.id)}
                        style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 4, background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent)', fontSize: 12, padding: 0, textAlign: 'left' }}
                        title={replyPreview}
                      >
                        <span>💬 {replyCount} {replyCount === 1 ? 'reply' : 'replies'}</span>
                        <span style={{ color: 'var(--text-muted)', fontStyle: 'italic', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 200 }}>{replyPreview}</span>
                      </button>
                    )}
                    {isShowingHistory && msgEdits.length > 0 && (
                      <div style={{ marginTop: 4, padding: '6px 10px', background: 'var(--surface)', borderRadius: 6, border: '1px solid var(--border)', fontSize: 12, color: 'var(--text-muted)' }}>
                        <div style={{ fontWeight: 600, marginBottom: 4 }}>Edit history:</div>
                        {msgEdits.map((e: MessageEdit) => (
                          <div key={String(e.id)} style={{ marginBottom: 2 }}>
                            <span style={{ color: 'var(--text-muted)', marginRight: 6 }}>{formatTime(e.editedAt)}</span>
                            <span style={{ textDecoration: 'line-through', color: 'var(--text-muted)' }}>{e.oldText}</span>
                            <span style={{ margin: '0 4px' }}>→</span>
                            <span>{e.newText}</span>
                          </div>
                        ))}
                      </div>
                    )}
                    {isEphemeralMsg && (
                      <div className="ephemeral-indicator" title="This message will disappear">
                        ⏳ Disappears in {formatCountdown(msg.expiresAtMicros)}
                      </div>
                    )}
                    {readBy.length > 0 && (
                      <div className="read-receipt">Seen by {readBy.join(', ')}</div>
                    )}
                    <div className="reactions-area">
                      {reactionGroups.map(rg => (
                        <button
                          key={rg.emoji}
                          className={`reaction-btn${rg.iMine ? ' mine' : ''}`}
                          title={rg.reactors.join(', ')}
                          onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, roomId: room.id, emoji: rg.emoji })}
                        >
                          {rg.emoji} {rg.count}
                        </button>
                      ))}
                      <div className="reaction-picker">
                        {REACTION_EMOJIS.map(emoji => (
                          <button
                            key={emoji}
                            className="reaction-add-btn"
                            title={`React with ${emoji}`}
                            onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, roomId: room.id, emoji })}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
          <div ref={messagesEndRef} />
        </div>
        {!isAtBottom && (
          <button className="scroll-btn" onClick={scrollToBottom} title="Scroll to bottom">↓</button>
        )}
      </div>
      <div className="typing-indicator">{typingText}</div>
      <div className="input-bar">
        {isEphemeral && (
          <div className="ephemeral-bar">
            <span style={{ color: 'var(--warning)', fontSize: 12 }}>⏳ Ephemeral:</span>
            <select
              value={ephemeralDuration}
              onChange={(e) => setEphemeralDuration(Number(e.target.value))}
              style={{ background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: 4, padding: '2px 6px', fontSize: 12 }}
            >
              <option value={30}>30 seconds</option>
              <option value={60}>1 minute</option>
              <option value={300}>5 minutes</option>
              <option value={1800}>30 minutes</option>
              <option value={3600}>1 hour</option>
            </select>
          </div>
        )}
        <div className="input-row">
          <input
            type="text"
            placeholder={isEphemeral ? `Ephemeral message (${ephemeralDuration}s)…` : 'Type a message...'}
            value={text}
            onChange={(e) => handleDraftChange(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
            maxLength={2000}
            style={isEphemeral ? { borderColor: 'var(--warning)' } : undefined}
          />
          <button className="primary" onClick={handleSend}>Send</button>
          <button
            className={isEphemeral ? 'primary' : 'secondary'}
            onClick={() => setIsEphemeral(!isEphemeral)}
            title={isEphemeral ? 'Disable ephemeral mode' : 'Send disappearing message'}
            style={{ whiteSpace: 'nowrap', background: isEphemeral ? 'var(--warning)' : undefined, color: isEphemeral ? '#000' : undefined }}
          >
            ⏳ Ephemeral
          </button>
          <button
            className="secondary"
            onClick={() => setShowScheduleModal(true)}
            title="Schedule message"
            style={{ whiteSpace: 'nowrap' }}
          >
            ⏰ Schedule
          </button>
        </div>
      </div>

      {showScheduleModal && (
        <ScheduleMessageModal
          conn={conn}
          roomId={room.id}
          onClose={() => setShowScheduleModal(false)}
        />
      )}

      {showInviteModal && (
        <InviteUserModal
          conn={conn}
          roomId={room.id}
          onClose={() => setShowInviteModal(false)}
        />
      )}
    </>
  );
}

// ---- Thread Panel ----
function ThreadPanel({
  parentMessage, replies, users, myHex, roomId, conn, onClose
}: {
  parentMessage: Message | null;
  replies: ReadonlyArray<ThreadReply>;
  users: ReadonlyArray<User>;
  myHex: string;
  roomId: bigint;
  conn: DbConnection | null;
  onClose: () => void;
}) {
  const [replyText, setReplyText] = useState('');
  const repliesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    repliesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [replies]);

  const handleSendReply = () => {
    const trimmed = replyText.trim();
    if (!trimmed || !parentMessage) return;
    conn?.reducers.replyToMessage({ parentMessageId: parentMessage.id, roomId, text: trimmed });
    setReplyText('');
  };

  if (!parentMessage) return null;

  const parentSender = users.find((u: User) => u.identity.toHexString() === parentMessage.sender.toHexString());

  return (
    <div className="thread-panel">
      <div className="thread-panel-header">
        <span style={{ fontWeight: 600 }}>Thread</span>
        <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 18, lineHeight: 1 }} title="Close thread">✕</button>
      </div>
      <div className="thread-panel-parent">
        <span className="message-sender" style={{ color: 'var(--accent)' }}>{parentSender?.name ?? 'Unknown'}</span>
        <span className="message-time" style={{ marginLeft: 8 }}>{formatTime(parentMessage.sentAt)}</span>
        <div className="message-text" style={{ marginTop: 4 }}>{parentMessage.text}</div>
      </div>
      <div className="thread-replies-list">
        {replies.length === 0 && (
          <div style={{ color: 'var(--text-muted)', fontSize: 13, padding: '12px 0' }}>No replies yet. Start the thread!</div>
        )}
        {replies.map((r: ThreadReply) => {
          const sender = users.find((u: User) => u.identity.toHexString() === r.sender.toHexString());
          const isMe = r.sender.toHexString() === myHex;
          return (
            <div key={String(r.id)} className="thread-reply-item">
              <span className={`message-sender${isMe ? ' me' : ''}`}>{sender?.name ?? 'Unknown'}</span>
              <span className="message-time" style={{ marginLeft: 8 }}>{formatTime(r.sentAt)}</span>
              <div className="message-text">{r.text}</div>
            </div>
          );
        })}
        <div ref={repliesEndRef} />
      </div>
      <div className="thread-reply-input">
        <input
          type="text"
          placeholder="Reply in thread..."
          value={replyText}
          onChange={(e) => setReplyText(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSendReply(); } }}
          maxLength={2000}
        />
        <button className="primary" onClick={handleSendReply} style={{ whiteSpace: 'nowrap' }}>Reply</button>
      </div>
    </div>
  );
}
