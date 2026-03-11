import { useEffect, useRef } from 'react';
import {
  DbConnection,
  Message,
  User,
  MessageReaction,
  ReadReceipt,
  MessageEdit,
} from '../module_bindings';
import { Identity } from 'spacetimedb/react';
import MessageItem from './MessageItem';
import MessageInput from './MessageInput';

interface ThreadPanelProps {
  conn: DbConnection;
  parentMessageId: bigint;
  myIdentity: Identity | null;
  users: User[];
  allMessages: Message[];
  reactions: MessageReaction[];
  readReceipts: ReadReceipt[];
  edits: MessageEdit[];
  onClose: () => void;
}

export default function ThreadPanel({
  conn,
  parentMessageId,
  myIdentity,
  users,
  allMessages,
  reactions,
  readReceipts,
  edits,
  onClose,
}: ThreadPanelProps) {
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const parentMessage = allMessages.find(m => m.id === parentMessageId);
  const replies = allMessages
    .filter(m => m.threadParentId === parentMessageId)
    .sort((a, b) =>
      Number(
        a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
      )
    );

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [replies.length]);

  if (!parentMessage) {
    return null;
  }

  return (
    <div className="thread-panel">
      <div className="thread-panel-header">
        <h3>Thread</h3>
        <button className="btn-icon" onClick={onClose}>
          ✕
        </button>
      </div>

      <div className="messages-container" style={{ flex: 1, overflow: 'auto' }}>
        {/* Parent message */}
        <div
          style={{
            borderBottom: '1px solid var(--border)',
            paddingBottom: '16px',
            marginBottom: '16px',
          }}
        >
          <MessageItem
            message={parentMessage}
            conn={conn}
            myIdentity={myIdentity}
            users={users}
            reactions={reactions.filter(r => r.messageId === parentMessage.id)}
            readReceipts={readReceipts.filter(
              r => r.messageId === parentMessage.id
            )}
            edits={edits.filter(e => e.messageId === parentMessage.id)}
            replyCount={0}
            onViewThread={() => {}}
            isMember={true}
          />
        </div>

        {/* Replies */}
        <div
          style={{
            fontSize: '12px',
            color: 'var(--text-muted)',
            marginBottom: '8px',
          }}
        >
          {replies.length} {replies.length === 1 ? 'reply' : 'replies'}
        </div>

        {replies.map(reply => (
          <MessageItem
            key={reply.id.toString()}
            message={reply}
            conn={conn}
            myIdentity={myIdentity}
            users={users}
            reactions={reactions.filter(r => r.messageId === reply.id)}
            readReceipts={readReceipts.filter(r => r.messageId === reply.id)}
            edits={edits.filter(e => e.messageId === reply.id)}
            replyCount={0}
            onViewThread={() => {}}
            isMember={true}
          />
        ))}
        <div ref={messagesEndRef} />
      </div>

      <MessageInput
        conn={conn}
        roomId={parentMessage.roomId}
        replyToId={parentMessageId}
      />
    </div>
  );
}
