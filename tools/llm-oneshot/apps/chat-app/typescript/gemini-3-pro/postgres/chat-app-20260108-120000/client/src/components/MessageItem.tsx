import React, { useState, useEffect } from 'react';
import { Message, Reaction, User } from '../types';
import { format, formatDistanceToNow } from 'date-fns';
import { Smile, Edit2, Check } from 'lucide-react';
import { useAuth } from '../App';
import clsx from 'clsx'; // Assuming I can import it, I added it to package.json

interface Props {
  message: Message;
  seenBy: string[]; // List of usernames who saw this
  onReact: (id: number, emoji: string) => void;
  onEdit: (id: number, content: string) => void;
}

export default function MessageItem({ message, seenBy, onReact, onEdit }: Props) {
  const { user } = useAuth();
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(message.content);
  const [timeLeft, setTimeLeft] = useState<string | null>(null);

  const isAuthor = user?.id === message.userId;

  // Ephemeral countdown
  useEffect(() => {
    if (!message.expiresAt) return;
    const interval = setInterval(() => {
      const end = new Date(message.expiresAt!);
      const now = new Date();
      const diff = end.getTime() - now.getTime();
      if (diff <= 0) {
        setTimeLeft('Expired');
        clearInterval(interval);
      } else {
        setTimeLeft(`${Math.ceil(diff / 1000)}s`);
      }
    }, 1000);
    return () => clearInterval(interval);
  }, [message.expiresAt]);

  const handleSaveEdit = () => {
    onEdit(message.id, editContent);
    setIsEditing(false);
  };

  const reactionCounts = message.reactions.reduce((acc, r) => {
    acc[r.emoji] = (acc[r.emoji] || 0) + 1;
    return acc;
  }, {} as Record<string, number>);

  const hasReacted = (emoji: string) => message.reactions.some(r => r.emoji === emoji && r.userId === user?.id);

  return (
    <div className="message-item" style={{ padding: '8px 16px', marginBottom: 4, position: 'relative' }}>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 8 }}>
        <span style={{ fontWeight: 'bold', color: 'white' }}>{message.author.username}</span>
        <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>
          {format(new Date(message.createdAt), 'p')}
        </span>
        {message.scheduledFor && (
          <span style={{ fontSize: 10, background: '#444', padding: '2px 4px', borderRadius: 4, color: '#aaa' }}>
            Scheduled for {format(new Date(message.scheduledFor), 'p')}
          </span>
        )}
        {message.expiresAt && (
          <span style={{ fontSize: 10, background: 'var(--danger)', padding: '2px 4px', borderRadius: 4, color: 'white' }}>
            {timeLeft ? `Expires in ${timeLeft}` : 'Expiring...'}
          </span>
        )}
      </div>

      {isEditing ? (
        <div style={{ marginTop: 4 }}>
          <input 
            className="input" 
            value={editContent} 
            onChange={e => setEditContent(e.target.value)} 
            onKeyDown={e => e.key === 'Enter' && handleSaveEdit()}
            autoFocus
          />
          <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 8 }}>Press Enter to save</span>
        </div>
      ) : (
        <div style={{ marginTop: 4, color: 'var(--text-normal)', whiteSpace: 'pre-wrap' }}>
          {message.content}
          {message.editedAt && (
             <span 
               style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 4, cursor: 'help' }}
               title={`Last edited: ${new Date(message.editedAt).toLocaleString()}\nHistory available in database.`}
             >
               (edited)
             </span>
          )}
        </div>
      )}

      {/* Reactions */}
      <div style={{ display: 'flex', gap: 4, marginTop: 4 }}>
        {Object.entries(reactionCounts).map(([emoji, count]) => (
          <button
            key={emoji}
            onClick={() => onReact(message.id, emoji)}
            style={{
              background: hasReacted(emoji) ? 'rgba(88, 101, 242, 0.3)' : 'var(--bg-tertiary)',
              border: hasReacted(emoji) ? '1px solid var(--accent)' : '1px solid transparent',
              borderRadius: 8,
              padding: '2px 6px',
              color: 'var(--text-normal)',
              cursor: 'pointer',
              fontSize: 12,
              display: 'flex',
              alignItems: 'center',
              gap: 4
            }}
          >
            <span>{emoji}</span>
            <span style={{ fontWeight: 'bold' }}>{count}</span>
          </button>
        ))}
      </div>

      {/* Read Receipts */}
      {seenBy.length > 0 && (
         <div style={{ fontSize: 10, color: 'var(--text-muted)', marginTop: 2, textAlign: 'right' }}>
           Seen by {seenBy.join(', ')}
         </div>
      )}

      {/* Actions (hover) */}
      <div className="message-actions" style={{ 
        position: 'absolute', 
        top: -10, 
        right: 16, 
        background: 'var(--bg-secondary)', 
        border: '1px solid var(--bg-tertiary)',
        borderRadius: 4,
        padding: 4,
        display: 'none', // Shown via CSS
        gap: 4
      }}>
        <button onClick={() => onReact(message.id, '👍')} className="btn" style={{ padding: 4 }} title="React 👍"><Smile size={14}/></button>
        <button onClick={() => onReact(message.id, '❤️')} className="btn" style={{ padding: 4 }} title="React ❤️"><span style={{fontSize:12}}>❤️</span></button>
        {isAuthor && (
          <button onClick={() => setIsEditing(!isEditing)} className="btn" style={{ padding: 4 }} title="Edit"><Edit2 size={14}/></button>
        )}
      </div>

      <style>{`
        .message-item:hover { background-color: var(--message-hover); }
        .message-item:hover .message-actions { display: flex; }
      `}</style>
    </div>
  );
}
