import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, Identity } from 'spacetimedb/react';
import { DbConnection, tables, User, RoomMember, Message } from '../module_bindings';
import MessageItem from './MessageItem';
import MessageInput from './MessageInput';
import MembersPanel from './MembersPanel';
import ThreadPanel from './ThreadPanel';
import RoomSettingsModal from './RoomSettingsModal';
import ScheduledMessagesPanel from './ScheduledMessagesPanel';

interface ChatAreaProps {
  conn: DbConnection;
  roomId: bigint;
  myIdentity: Identity | null;
  users: User[];
  roomMembers: RoomMember[];
}

export default function ChatArea({ conn, roomId, myIdentity, users, roomMembers }: ChatAreaProps) {
  const [showMembers, setShowMembers] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showScheduled, setShowScheduled] = useState(false);
  const [selectedThreadId, setSelectedThreadId] = useState<bigint | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const [rooms] = useTable(tables.room);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [reactions] = useTable(tables.messageReaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [roomBans] = useTable(tables.roomBan);

  const room = rooms?.find(r => r.id === roomId);
  const roomMemberList = roomMembers.filter(m => m.roomId === roomId);
  const roomMessageList = messages?.filter(m => m.roomId === roomId && m.threadParentId == null) ?? [];
  const sortedMessages = [...roomMessageList].sort(
    (a, b) => Number(a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch)
  );

  const myMembership = roomMemberList.find(
    m => myIdentity && m.userId.toHexString() === myIdentity.toHexString()
  );
  const isMember = !!myMembership;
  const isAdmin = myMembership?.isAdmin ?? false;

  const typingUsers = typingIndicators
    ?.filter(t => t.roomId === roomId && myIdentity && t.userId.toHexString() !== myIdentity.toHexString())
    .map(t => users.find(u => u.identity.toHexString() === t.userId.toHexString())?.name ?? 'Someone') ?? [];

  const myScheduledMessages = scheduledMessages?.filter(
    sm => sm.roomId === roomId && myIdentity && sm.senderId.toHexString() === myIdentity.toHexString()
  ) ?? [];

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [sortedMessages.length]);

  // Mark messages as read
  useEffect(() => {
    if (!isMember || !myIdentity || sortedMessages.length === 0) return;

    const lastMessage = sortedMessages[sortedMessages.length - 1];
    const alreadyRead = readReceipts?.some(
      r => r.messageId === lastMessage.id && r.userId.toHexString() === myIdentity.toHexString()
    );

    if (!alreadyRead) {
      conn.reducers.markMessagesRead({ roomId, upToMessageId: lastMessage.id });
    }
  }, [conn, roomId, sortedMessages, myIdentity, isMember, readReceipts]);

  const handleJoinRoom = () => {
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = () => {
    conn.reducers.leaveRoom({ roomId });
  };

  // Count replies for thread indicators
  const getReplyCount = (messageId: bigint): number => {
    return messages?.filter(m => m.threadParentId === messageId).length ?? 0;
  };

  if (!room) {
    return (
      <div className="chat-area">
        <div className="no-room-selected">
          <p>Room not found</p>
        </div>
      </div>
    );
  }

  return (
    <div className="chat-area" style={{ display: 'flex' }}>
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
        <div className="chat-header">
          <h2>
            {room.isDm ? '💬' : room.isPrivate ? '🔒' : '#'} {room.name}
          </h2>
          <div className="chat-header-actions">
            {myScheduledMessages.length > 0 && (
              <button
                className="btn btn-secondary btn-small"
                onClick={() => setShowScheduled(!showScheduled)}
              >
                📅 {myScheduledMessages.length}
              </button>
            )}
            {isMember ? (
              <>
                {isAdmin && (
                  <button className="btn btn-secondary btn-small" onClick={() => setShowSettings(true)}>
                    ⚙️ Settings
                  </button>
                )}
                <button className="btn-icon" onClick={() => setShowMembers(!showMembers)}>
                  👥
                </button>
                <button className="btn btn-secondary btn-small" onClick={handleLeaveRoom}>
                  Leave
                </button>
              </>
            ) : (
              <button className="btn btn-primary btn-small" onClick={handleJoinRoom}>
                Join Room
              </button>
            )}
          </div>
        </div>

        {showScheduled && myScheduledMessages.length > 0 && (
          <ScheduledMessagesPanel
            conn={conn}
            scheduledMessages={myScheduledMessages}
            onClose={() => setShowScheduled(false)}
          />
        )}

        <div className="messages-container">
          {sortedMessages.length === 0 ? (
            <div style={{ textAlign: 'center', color: 'var(--text-muted)', padding: '32px' }}>
              No messages yet. Start the conversation!
            </div>
          ) : (
            sortedMessages.map(message => (
              <MessageItem
                key={message.id.toString()}
                message={message}
                conn={conn}
                myIdentity={myIdentity}
                users={users}
                reactions={reactions?.filter(r => r.messageId === message.id) ?? []}
                readReceipts={readReceipts?.filter(r => r.messageId === message.id) ?? []}
                edits={messageEdits?.filter(e => e.messageId === message.id) ?? []}
                replyCount={getReplyCount(message.id)}
                onViewThread={() => setSelectedThreadId(message.id)}
                isMember={isMember}
              />
            ))
          )}
          <div ref={messagesEndRef} />
        </div>

        {typingUsers.length > 0 && (
          <div className="typing-indicator">
            {typingUsers.length === 1
              ? `${typingUsers[0]} is typing...`
              : typingUsers.length === 2
              ? `${typingUsers[0]} and ${typingUsers[1]} are typing...`
              : 'Several people are typing...'}
          </div>
        )}

        {isMember && (
          <MessageInput conn={conn} roomId={roomId} />
        )}
      </div>

      {showMembers && isMember && (
        <MembersPanel
          conn={conn}
          roomId={roomId}
          members={roomMemberList}
          users={users}
          bans={roomBans?.filter(b => b.roomId === roomId) ?? []}
          isAdmin={isAdmin}
          myIdentity={myIdentity}
        />
      )}

      {selectedThreadId != null && (
        <ThreadPanel
          conn={conn}
          parentMessageId={selectedThreadId}
          myIdentity={myIdentity}
          users={users}
          allMessages={messages ?? []}
          reactions={reactions ?? []}
          readReceipts={readReceipts ?? []}
          edits={messageEdits ?? []}
          onClose={() => setSelectedThreadId(null)}
        />
      )}

      {showSettings && (
        <RoomSettingsModal
          conn={conn}
          room={room}
          members={roomMemberList}
          users={users}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}
