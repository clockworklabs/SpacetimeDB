import React, { useEffect, useMemo, useRef, useState } from 'react';
import { type Identity } from 'spacetimedb';
import { useTable } from 'spacetimedb/react';

import { DbConnection, tables } from './module_bindings';

type Props = {
  conn: DbConnection | null;
  identity: Identity | null;
  connectError: string | null;
};

const ALLOWED_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'] as const;

function identityEq(a: Identity, b: Identity) {
  return a.toHexString() === b.toHexString();
}

function microsToDate(micros: bigint) {
  return new Date(Number(micros / 1000n));
}

function formatTime(micros: bigint) {
  const d = microsToDate(micros);
  return d.toLocaleString(undefined, { hour: '2-digit', minute: '2-digit', month: 'short', day: '2-digit' });
}

function formatCountdownSeconds(nowMicros: bigint, targetMicros: bigint) {
  if (targetMicros <= nowMicros) return '0s';
  const s = Number((targetMicros - nowMicros) / 1_000_000n);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}m ${rem}s`;
}

function localDateTimeInputFromNow(now = new Date()) {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(
    now.getMinutes(),
  )}`;
}

function parseLocalDateTimeInputToMicros(value: string): bigint | null {
  if (!value) return null;
  const d = new Date(value);
  const ms = d.getTime();
  if (!Number.isFinite(ms)) return null;
  return BigInt(ms) * 1000n;
}

function useNowMicros(intervalMs = 250) {
  const [now, setNow] = useState(() => BigInt(Date.now()) * 1000n);
  useEffect(() => {
    const id = setInterval(() => setNow(BigInt(Date.now()) * 1000n), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="modalBackdrop" role="dialog" aria-modal="true" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="panelHeader">
          <h2>{title}</h2>
          <button className="btn btnSmall" onClick={onClose}>
            Close
          </button>
        </div>
        <div className="modalBody">{children}</div>
        <div className="panelBody">
          <button className="btn" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

export function App({ conn, identity, connectError }: Props) {
  const nowMicros = useNowMicros(250);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [reactions] = useTable(tables.reaction);
  const [readReceipts] = useTable(tables.readReceipt);
  const [roomReadPositions] = useTable(tables.roomReadPosition);
  const [messageEdits] = useTable(tables.messageEdit);

  type ReadReceiptRow = (typeof readReceipts)[number];
  type ReactionRow = (typeof reactions)[number];
  type MessageEditRow = (typeof messageEdits)[number];

  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [status, setStatus] = useState<{ kind: 'error' | 'success'; text: string } | null>(null);

  const [displayNameDraft, setDisplayNameDraft] = useState('');
  const [roomNameDraft, setRoomNameDraft] = useState('');

  const [messageDraft, setMessageDraft] = useState('');
  const [sendMode, setSendMode] = useState<'normal' | 'scheduled' | 'ephemeral'>('normal');
  const [scheduledAtDraft, setScheduledAtDraft] = useState(() => localDateTimeInputFromNow(new Date(Date.now() + 5 * 60_000)));
  const [ephemeralTtl, setEphemeralTtl] = useState(60);

  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editingDraft, setEditingDraft] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<bigint | null>(null);

  const lastTypingSentAtRef = useRef<number>(0);

  const me = useMemo(() => {
    if (!identity) return null;
    for (const u of users) {
      if (identityEq(u.identity, identity)) return u;
    }
    return null;
  }, [users, identity]);

  const onlineUsers = useMemo(() => users.filter((u) => u.online).sort((a, b) => a.displayName.localeCompare(b.displayName)), [users]);

  const membershipByRoomId = useMemo(() => {
    const m = new Map<bigint, boolean>();
    if (!identity) return m;
    for (const rm of roomMembers) {
      if (identityEq(rm.identity, identity)) m.set(rm.roomId, true);
    }
    return m;
  }, [roomMembers, identity]);

  const selectedRoom = useMemo(() => {
    if (selectedRoomId == null) return null;
    return rooms.find((r) => r.id === selectedRoomId) ?? null;
  }, [rooms, selectedRoomId]);

  const roomMessages = useMemo(() => {
    if (selectedRoomId == null) return [];
    const filtered = messages.filter((m) => m.roomId === selectedRoomId);
    filtered.sort((a, b) => (a.createdAtMicros < b.createdAtMicros ? -1 : a.createdAtMicros > b.createdAtMicros ? 1 : 0));
    return filtered;
  }, [messages, selectedRoomId]);

  const scheduledForSelectedRoom = useMemo(() => {
    if (!identity || selectedRoomId == null) return [];
    const filtered = scheduledMessages.filter(
      (sm) => sm.roomId === selectedRoomId && identityEq(sm.author, identity),
    );
    filtered.sort((a, b) => (a.scheduledAtMicros < b.scheduledAtMicros ? -1 : a.scheduledAtMicros > b.scheduledAtMicros ? 1 : 0));
    return filtered;
  }, [scheduledMessages, selectedRoomId, identity]);

  const receiptsByMessageId = useMemo(() => {
    const map = new Map<bigint, ReadReceiptRow[]>();
    for (const rr of readReceipts) {
      const arr = map.get(rr.messageId);
      if (arr) arr.push(rr);
      else map.set(rr.messageId, [rr]);
    }
    for (const arr of map.values()) {
      arr.sort((a, b) => (a.readAtMicros < b.readAtMicros ? -1 : a.readAtMicros > b.readAtMicros ? 1 : 0));
    }
    return map;
  }, [readReceipts]);

  const reactionsByMessageId = useMemo(() => {
    const byMsg = new Map<bigint, Map<string, ReactionRow[]>>();
    for (const r of reactions) {
      const byEmoji = byMsg.get(r.messageId) ?? new Map<string, ReactionRow[]>();
      const arr = byEmoji.get(r.emoji);
      if (arr) arr.push(r);
      else byEmoji.set(r.emoji, [r]);
      byMsg.set(r.messageId, byEmoji);
    }
    return byMsg;
  }, [reactions]);

  const editsByMessageId = useMemo(() => {
    const map = new Map<bigint, MessageEditRow[]>();
    for (const e of messageEdits) {
      const arr = map.get(e.messageId);
      if (arr) arr.push(e);
      else map.set(e.messageId, [e]);
    }
    for (const arr of map.values()) {
      arr.sort((a, b) => (a.editedAtMicros < b.editedAtMicros ? -1 : a.editedAtMicros > b.editedAtMicros ? 1 : 0));
    }
    return map;
  }, [messageEdits]);

  const lastReadByRoomId = useMemo(() => {
    const map = new Map<bigint, bigint>();
    if (!identity) return map;
    for (const rp of roomReadPositions) {
      if (!identityEq(rp.identity, identity)) continue;
      map.set(rp.roomId, rp.lastReadAtMicros);
    }
    return map;
  }, [roomReadPositions, identity]);

  const unreadCountByRoomId = useMemo(() => {
    const map = new Map<bigint, number>();
    for (const r of rooms) {
      const lastReadAt = lastReadByRoomId.get(r.id) ?? 0n;
      let count = 0;
      for (const m of messages) {
        if (m.roomId !== r.id) continue;
        if (m.createdAtMicros > lastReadAt) {
          // Don't count own messages as unread
          if (identity && identityEq(m.author, identity)) continue;
          count++;
        }
      }
      map.set(r.id, count);
    }
    return map;
  }, [rooms, messages, lastReadByRoomId, identity]);

  const typingInSelectedRoom = useMemo(() => {
    if (!identity || selectedRoomId == null) return [];
    return typingIndicators
      .filter((ti) => ti.roomId === selectedRoomId && ti.expiresAtMicros > nowMicros && !identityEq(ti.identity, identity))
      .map((ti) => ti.identity);
  }, [typingIndicators, selectedRoomId, identity, nowMicros]);

  const canSendInSelectedRoom = useMemo(() => {
    if (!identity || selectedRoomId == null) return false;
    return membershipByRoomId.get(selectedRoomId) === true;
  }, [membershipByRoomId, identity, selectedRoomId]);

  const messageListRef = useRef<HTMLDivElement | null>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);

  useEffect(() => {
    const el = messageListRef.current;
    if (!el) return;
    const onScroll = () => {
      const threshold = 60;
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
      setIsAtBottom(atBottom);
    };
    onScroll();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, [selectedRoomId]);

  useEffect(() => {
    if (!messageListRef.current) return;
    if (!isAtBottom) return;
    messageListRef.current.scrollTop = messageListRef.current.scrollHeight;
  }, [roomMessages.length, isAtBottom]);

  useEffect(() => {
    if (!conn || !selectedRoomId || !identity) return;
    if (!isAtBottom) return;
    const t = setTimeout(() => {
      conn.reducers.markRoomRead({ roomId: selectedRoomId });
    }, 200);
    return () => clearTimeout(t);
  }, [conn, selectedRoomId, identity, isAtBottom, roomMessages.length]);

  useEffect(() => {
    if (connectError) setStatus({ kind: 'error', text: connectError });
  }, [connectError]);

  const userNameByHex = useMemo(() => {
    const map = new Map<string, string>();
    for (const u of users) map.set(u.identity.toHexString(), u.displayName);
    return map;
  }, [users]);

  const showError = (text: string) => setStatus({ kind: 'error', text });
  const showSuccess = (text: string) => setStatus({ kind: 'success', text });

  const handleSetName = async () => {
    if (!conn) return;
    try {
      await conn.reducers.setName({ name: displayNameDraft });
      setDisplayNameDraft('');
      showSuccess('Updated display name');
    } catch (e: any) {
      showError(e?.message || 'Failed to set name');
    }
  };

  const handleCreateRoom = async () => {
    if (!conn) return;
    try {
      await conn.reducers.createRoom({ name: roomNameDraft });
      setRoomNameDraft('');
      showSuccess('Room created');
    } catch (e: any) {
      showError(e?.message || 'Failed to create room');
    }
  };

  const handleJoinRoom = async (roomId: bigint) => {
    if (!conn) return;
    try {
      await conn.reducers.joinRoom({ roomId });
      showSuccess('Joined room');
    } catch (e: any) {
      showError(e?.message || 'Failed to join room');
    }
  };

  const handleLeaveRoom = async (roomId: bigint) => {
    if (!conn) return;
    try {
      await conn.reducers.leaveRoom({ roomId });
      showSuccess('Left room');
    } catch (e: any) {
      showError(e?.message || 'Failed to leave room');
    }
  };

  const handleSend = async () => {
    if (!conn || selectedRoomId == null) return;
    if (!canSendInSelectedRoom) return showError('Join the room to send messages');

    try {
      if (sendMode === 'normal') {
        await conn.reducers.sendMessage({ roomId: selectedRoomId, content: messageDraft });
        setMessageDraft('');
      } else if (sendMode === 'ephemeral') {
        await conn.reducers.sendEphemeralMessage({ roomId: selectedRoomId, content: messageDraft, ttlSeconds: BigInt(ephemeralTtl) });
        setMessageDraft('');
      } else {
        const micros = parseLocalDateTimeInputToMicros(scheduledAtDraft);
        if (!micros) return showError('Invalid scheduled time');
        await conn.reducers.scheduleMessage({ roomId: selectedRoomId, content: messageDraft, scheduledAtMicros: micros });
        setMessageDraft('');
        showSuccess('Scheduled message');
      }
    } catch (e: any) {
      showError(e?.message || 'Failed to send');
    }
  };

  const handleStartTyping = () => {
    if (!conn || !selectedRoomId || !canSendInSelectedRoom) return;
    const now = Date.now();
    if (now - lastTypingSentAtRef.current < 900) return;
    lastTypingSentAtRef.current = now;
    conn.reducers.startTyping({ roomId: selectedRoomId });
  };

  const handleToggleReaction = async (messageId: bigint, emoji: string) => {
    if (!conn) return;
    try {
      await conn.reducers.toggleReaction({ messageId, emoji });
    } catch (e: any) {
      showError(e?.message || 'Failed to react');
    }
  };

  const startEditing = (messageId: bigint, current: string) => {
    setEditingMessageId(messageId);
    setEditingDraft(current);
  };

  const cancelEditing = () => {
    setEditingMessageId(null);
    setEditingDraft('');
  };

  const saveEditing = async () => {
    if (!conn || editingMessageId == null) return;
    try {
      await conn.reducers.editMessage({ messageId: editingMessageId, newContent: editingDraft });
      cancelEditing();
      showSuccess('Message edited');
    } catch (e: any) {
      showError(e?.message || 'Failed to edit message');
    }
  };

  const cancelScheduled = async (scheduledMessageId: bigint) => {
    if (!conn) return;
    try {
      await conn.reducers.cancelScheduledMessage({ scheduledMessageId });
      showSuccess('Cancelled scheduled message');
    } catch (e: any) {
      showError(e?.message || 'Failed to cancel scheduled message');
    }
  };

  const statusClass =
    status?.kind === 'error' ? 'statusBar statusError' : status?.kind === 'success' ? 'statusBar statusSuccess' : 'statusBar';

  return (
    <div className="app">
      <div className="panel">
        <div className="panelHeader">
          <h2>Rooms</h2>
          <span className="muted">{identity ? me?.displayName ?? '...' : 'Connecting...'}</span>
        </div>
        <div className="panelBody stack">
          {status ? <div className={statusClass}>{status.text}</div> : null}

          <div className="stack">
            <div className="row">
              <input
                className="input"
                placeholder="Set display name…"
                value={displayNameDraft}
                onChange={(e) => setDisplayNameDraft(e.target.value)}
              />
              <button className="btn btnPrimary" onClick={handleSetName} disabled={!conn}>
                Save
              </button>
            </div>

            <div className="row">
              <input
                className="input"
                placeholder="Create a room…"
                value={roomNameDraft}
                onChange={(e) => setRoomNameDraft(e.target.value)}
              />
              <button className="btn btnPrimary" onClick={handleCreateRoom} disabled={!conn}>
                Create
              </button>
            </div>
          </div>

          <div className="divider" />

          {rooms.length === 0 ? (
            <div className="muted">No rooms yet. Create one to get started.</div>
          ) : (
            <div className="roomList">
              {rooms
                .slice()
                .sort((a, b) => (a.createdAtMicros < b.createdAtMicros ? -1 : a.createdAtMicros > b.createdAtMicros ? 1 : 0))
                .map((r) => {
                  const active = selectedRoomId === r.id;
                  const joined = membershipByRoomId.get(r.id) === true;
                  const unread = unreadCountByRoomId.get(r.id) ?? 0;
                  return (
                    <div
                      key={r.id.toString()}
                      className={`roomItem ${active ? 'roomItemActive' : ''}`}
                      onClick={() => setSelectedRoomId(r.id)}
                      role="button"
                      tabIndex={0}
                    >
                      <div className="row">
                        <div>
                          <div className="roomName">#{r.name}</div>
                          <div className="muted" style={{ fontSize: 12 }}>
                            {joined ? 'Joined' : 'Not joined'}
                          </div>
                        </div>
                        {unread > 0 ? <span className="badge">{unread}</span> : null}
                      </div>
                      <div className="row">
                        {joined ? (
                          <button
                            className="btn btnSmall"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleLeaveRoom(r.id);
                            }}
                            disabled={!conn}
                          >
                            Leave
                          </button>
                        ) : (
                          <button
                            className="btn btnSmall btnPrimary"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleJoinRoom(r.id);
                            }}
                            disabled={!conn}
                          >
                            Join
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
            </div>
          )}
        </div>
      </div>

      <div className="panel messagePane">
        <div className="panelHeader">
          <h2>{selectedRoom ? `#${selectedRoom.name}` : 'Chat'}</h2>
          <span className="muted">{selectedRoom ? (canSendInSelectedRoom ? 'You can chat' : 'Join to chat') : ''}</span>
        </div>

        <div className="messages" ref={messageListRef}>
          {!selectedRoom ? (
            <div className="muted">Select a room to view messages.</div>
          ) : roomMessages.length === 0 ? (
            <div className="muted">No messages yet. Say hello.</div>
          ) : (
            roomMessages.map((m) => {
              const authorName = userNameByHex.get(m.author.toHexString()) ?? m.author.toHexString().slice(0, 10);
              const mine = identity ? identityEq(m.author, identity) : false;
              const edited = m.editedAtMicros != null;

              const rxn = reactionsByMessageId.get(m.id);
              const receipts = receiptsByMessageId.get(m.id) ?? [];
              const myHex = identity?.toHexString();
              const seenNames = receipts
                .filter((rr) => rr.identity.toHexString() !== myHex) // Don't show yourself
                .map((rr) => userNameByHex.get(rr.identity.toHexString()) ?? rr.identity.toHexString().slice(0, 10))
                .filter((n) => n !== authorName);

              const isEditing = editingMessageId === m.id;
              const expiresAt = m.expiresAtMicros ?? null;

              return (
                <div key={m.id.toString()} className="messageRow">
                  <div className="messageHeader">
                    <div className="messageMeta">
                      <div className="messageAuthor">{authorName}</div>
                      <div className="messageTime">
                        {formatTime(m.createdAtMicros)}
                        {edited ? <span className="muted"> (edited)</span> : null}
                        {m.isEphemeral && expiresAt ? (
                          <span className="muted"> · disappears in {formatCountdownSeconds(nowMicros, expiresAt)}</span>
                        ) : null}
                      </div>
                    </div>
                    <div className="row">
                      {mine && !m.isEphemeral ? (
                        <button className="btn btnSmall" onClick={() => startEditing(m.id, m.content)}>
                          Edit
                        </button>
                      ) : null}
                      {edited ? (
                        <button className="btn btnSmall" onClick={() => setHistoryMessageId(m.id)}>
                          History
                        </button>
                      ) : null}
                    </div>
                  </div>

                  {isEditing ? (
                    <div className="stack" style={{ marginTop: 8 }}>
                      <textarea
                        className="input"
                        style={{ minHeight: 72, resize: 'vertical' }}
                        value={editingDraft}
                        onChange={(e) => setEditingDraft(e.target.value)}
                      />
                      <div className="row">
                        <button className="btn btnPrimary" onClick={saveEditing}>
                          Save
                        </button>
                        <button className="btn" onClick={cancelEditing}>
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="messageContent">{m.content}</div>
                  )}

                  <div className="messageFooter">
                    <div className="pillRow">
                      {ALLOWED_EMOJIS.map((emoji) => {
                        const arr = rxn?.get(emoji) ?? [];
                        const count = arr.length;
                        const who = arr
                          .map((r) => userNameByHex.get(r.identity.toHexString()) ?? r.identity.toHexString().slice(0, 10))
                          .join(', ');
                        const label = count > 0 ? `${emoji} ${count}` : emoji;
                        return (
                          <button
                            key={emoji}
                            className="pill"
                            title={who ? `Reacted: ${who}` : 'React'}
                            onClick={() => handleToggleReaction(m.id, emoji)}
                            disabled={!conn || !selectedRoom || !canSendInSelectedRoom}
                          >
                            <span>{label}</span>
                          </button>
                        );
                      })}
                    </div>

                    <div className="muted" style={{ fontSize: 12, textAlign: 'right' }}>
                      {seenNames.length > 0 ? `Seen by ${seenNames.slice(0, 4).join(', ')}${seenNames.length > 4 ? '…' : ''}` : ''}
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>

        <div className="composer">
          {selectedRoom ? (
            <div className="composerTop">
              {typingInSelectedRoom.length > 0 ? (
                <div className="muted">
                  {typingInSelectedRoom.length === 1
                    ? `${userNameByHex.get(typingInSelectedRoom[0]!.toHexString()) ?? 'Someone'} is typing…`
                    : `${typingInSelectedRoom.length} people are typing…`}
                </div>
              ) : null}

              <textarea
                className="input"
                style={{ minHeight: 80, resize: 'vertical' }}
                placeholder={canSendInSelectedRoom ? 'Write a message…' : 'Join the room to send messages…'}
                value={messageDraft}
                onChange={(e) => {
                  setMessageDraft(e.target.value);
                  handleStartTyping();
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) handleSend();
                }}
                disabled={!canSendInSelectedRoom}
              />

              <div className="composerActions">
                <div className="rowWrap">
                  <select className="select" value={sendMode} onChange={(e) => setSendMode(e.target.value as any)}>
                    <option value="normal">Send now</option>
                    <option value="scheduled">Schedule</option>
                    <option value="ephemeral">Ephemeral</option>
                  </select>

                  {sendMode === 'scheduled' ? (
                    <input
                      className="input"
                      type="datetime-local"
                      value={scheduledAtDraft}
                      min={localDateTimeInputFromNow(new Date(Date.now() + 60_000))}
                      onChange={(e) => setScheduledAtDraft(e.target.value)}
                      title="Local time"
                    />
                  ) : null}

                  {sendMode === 'ephemeral' ? (
                    <select className="select" value={ephemeralTtl} onChange={(e) => setEphemeralTtl(Number(e.target.value))}>
                      <option value={60}>Disappear in 1 minute</option>
                      <option value={300}>Disappear in 5 minutes</option>
                      <option value={900}>Disappear in 15 minutes</option>
                    </select>
                  ) : null}
                </div>

                <button className="btn btnPrimary" onClick={handleSend} disabled={!conn || !canSendInSelectedRoom || !messageDraft.trim()}>
                  {sendMode === 'scheduled' ? 'Schedule' : sendMode === 'ephemeral' ? 'Send (ephemeral)' : 'Send'}
                </button>
              </div>

              {scheduledForSelectedRoom.length > 0 ? (
                <div className="statusBar">
                  <div className="row" style={{ justifyContent: 'space-between' }}>
                    <strong>Scheduled (yours)</strong>
                    <span className="muted">{scheduledForSelectedRoom.length}</span>
                  </div>
                  <div className="stack" style={{ marginTop: 10 }}>
                    {scheduledForSelectedRoom.slice(0, 5).map((sm) => (
                      <div key={sm.id.toString()} className="row" style={{ justifyContent: 'space-between' }}>
                        <div style={{ minWidth: 0 }}>
                          <div style={{ fontSize: 13, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                            {sm.content}
                          </div>
                          <div className="muted" style={{ fontSize: 12 }}>
                            Sends at {formatTime(sm.scheduledAtMicros)}
                          </div>
                        </div>
                        <button className="btn btnSmall btnDanger" onClick={() => cancelScheduled(sm.id)} disabled={!conn}>
                          Cancel
                        </button>
                      </div>
                    ))}
                    {scheduledForSelectedRoom.length > 5 ? <div className="muted">…and more</div> : null}
                  </div>
                </div>
              ) : null}
            </div>
          ) : (
            <div className="muted">Pick a room to start chatting.</div>
          )}
        </div>
      </div>

      <div className="panel rightPanel">
        <div className="panelHeader">
          <h2>Online</h2>
          <span className="muted">{onlineUsers.length}</span>
        </div>
        <div className="panelBody">
          {onlineUsers.length === 0 ? (
            <div className="muted">No one online yet.</div>
          ) : (
            <div className="stack">
              {onlineUsers.map((u) => (
                <div key={u.id.toString()} className="row" style={{ justifyContent: 'space-between' }}>
                  <div>
                    <strong>{u.displayName}</strong>
                    <div className="muted" style={{ fontSize: 12 }}>
                      seen {formatTime(u.lastSeenMicros)}
                    </div>
                  </div>
                  <span className="badge" title={u.identity.toHexString()}>
                    ●
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {historyMessageId != null ? (
        <Modal title="Edit history" onClose={() => setHistoryMessageId(null)}>
          {(() => {
            const edits = editsByMessageId.get(historyMessageId) ?? [];
            if (edits.length === 0) return <div className="muted">No history available.</div>;
            return (
              <div className="stack">
                {edits.map((e) => {
                  const editorName = userNameByHex.get(e.editor.toHexString()) ?? e.editor.toHexString().slice(0, 10);
                  return (
                    <div key={e.id.toString()} className="statusBar">
                      <div className="row" style={{ justifyContent: 'space-between' }}>
                        <strong>{editorName}</strong>
                        <span className="muted">{formatTime(e.editedAtMicros)}</span>
                      </div>
                      <div className="divider" />
                      <div className="muted" style={{ fontSize: 12 }}>
                        Before
                      </div>
                      <div style={{ whiteSpace: 'pre-wrap' }}>{e.oldContent}</div>
                      <div className="divider" />
                      <div className="muted" style={{ fontSize: 12 }}>
                        After
                      </div>
                      <div style={{ whiteSpace: 'pre-wrap' }}>{e.newContent}</div>
                    </div>
                  );
                })}
              </div>
            );
          })()}
        </Modal>
      ) : null}
    </div>
  );
}

