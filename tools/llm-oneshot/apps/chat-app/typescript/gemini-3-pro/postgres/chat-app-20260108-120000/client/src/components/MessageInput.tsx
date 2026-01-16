import React, { useState, useEffect } from 'react';
import { Send, Clock, Timer } from 'lucide-react';
import { socket } from '../socket';

interface MessageInputProps {
  roomId: number;
}

export default function MessageInput({ roomId }: MessageInputProps) {
  const [content, setContent] = useState('');
  const [scheduledFor, setScheduledFor] = useState('');
  const [expiresIn, setExpiresIn] = useState('');
  const [showOptions, setShowOptions] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim()) return;

    const token = localStorage.getItem('token');
    try {
      await fetch(`/api/rooms/${roomId}/messages`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${token}`
        },
        body: JSON.stringify({
          content,
          scheduledFor: scheduledFor ? new Date(scheduledFor).toISOString() : undefined,
          expiresInSeconds: expiresIn ? parseInt(expiresIn) : undefined
        })
      });
      setContent('');
      setScheduledFor('');
      setExpiresIn('');
      setShowOptions(false);
      socket.emit('typing:stop', roomId);
    } catch (err) {
      console.error(err);
    }
  };

  const handleTyping = (e: React.ChangeEvent<HTMLInputElement>) => {
    setContent(e.target.value);
    socket.emit('typing:start', roomId);
    
    // Debounce stop
    const timeoutId = setTimeout(() => {
      socket.emit('typing:stop', roomId);
    }, 2000);
    return () => clearTimeout(timeoutId);
  };

  return (
    <div style={{ padding: 16, borderTop: '1px solid var(--bg-tertiary)' }}>
      {showOptions && (
        <div style={{ marginBottom: 10, padding: 10, background: 'var(--bg-tertiary)', borderRadius: 4 }}>
          <div style={{ display: 'flex', gap: 10, marginBottom: 5 }}>
            <label style={{ flex: 1 }}>
              <span style={{ fontSize: 12, display: 'block', color: 'var(--text-muted)' }}>Schedule for</span>
              <input 
                type="datetime-local" 
                className="input" 
                style={{ width: '100%', fontSize: 14 }}
                value={scheduledFor}
                onChange={e => setScheduledFor(e.target.value)}
              />
            </label>
            <label style={{ flex: 1 }}>
              <span style={{ fontSize: 12, display: 'block', color: 'var(--text-muted)' }}>Expires in (seconds)</span>
              <input 
                type="number" 
                className="input" 
                style={{ width: '100%', fontSize: 14 }}
                value={expiresIn}
                onChange={e => setExpiresIn(e.target.value)}
                placeholder="e.g. 60"
              />
            </label>
          </div>
        </div>
      )}
      <form onSubmit={handleSubmit} style={{ display: 'flex', gap: 10 }}>
        <button 
          type="button" 
          className="btn" 
          onClick={() => setShowOptions(!showOptions)}
          style={{ background: showOptions ? 'var(--bg-tertiary)' : 'transparent', color: 'var(--text-muted)' }}
          title="Message Options"
        >
          <Clock size={20} />
        </button>
        <input
          className="input"
          style={{ flex: 1 }}
          placeholder={`Message #${roomId}`}
          value={content}
          onChange={handleTyping}
        />
        <button className="btn btn-primary" type="submit">
          <Send size={18} />
        </button>
      </form>
    </div>
  );
}
