import React, { useEffect, useState, useRef } from 'react';
import { useParams } from 'react-router-dom';
import { socket } from '../socket';
import { Message, User } from '../types';
import MessageItem from './MessageItem';
import MessageInput from './MessageInput';
import { useAuth } from '../App';

export default function ChatRoom() {
  const { roomId } = useParams<{ roomId: string }>();
  const { user } = useAuth();
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(true);
  const [typingUsers, setTypingUsers] = useState<Set<string>>(new Set());
  const [readStatus, setReadStatus] = useState<Record<string, number>>({}); // userId -> messageId
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const parsedRoomId = parseInt(roomId!);

  useEffect(() => {
    if (!roomId) return;
    setLoading(true);
    
    // Fetch messages
    const token = localStorage.getItem('token');
    fetch(`/api/rooms/${roomId}/messages`, { headers: { Authorization: `Bearer ${token}` } })
      .then(res => res.json())
      .then(data => {
        setMessages(data);
        setLoading(false);
        scrollToBottom();
        // Initial read status update
        if (data.length > 0) {
          markAsRead(data[data.length - 1].id);
        }
      })
      .catch(e => {
        console.error(e);
        setLoading(false);
      });

    // Join room
    socket.emit('room:join', roomId);

    // Socket listeners
    const onMessageCreated = (msg: Message) => {
      setMessages(prev => {
        // Dedup
        if (prev.find(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      // If we are near bottom, scroll?
      // For now, simple auto-scroll if it's my message or I'm at bottom
      // But also mark as read immediately if focused?
      if (user) {
        markAsRead(msg.id);
      }
    };

    const onMessageUpdated = (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? msg : m));
    };

    const onMessageDeleted = ({ id }: { id: number }) => {
      setMessages(prev => prev.filter(m => m.id !== id));
    };

    const onTypingStarted = ({ username }: { username: string }) => {
      setTypingUsers(prev => new Set(prev).add(username));
    };

    const onTypingStopped = ({ username }: { username: string }) => {
      setTypingUsers(prev => {
        const next = new Set(prev);
        next.delete(username);
        return next;
      });
    };

    const onReadUpdated = ({ userId, lastReadMessageId }: { userId: string, lastReadMessageId: number }) => {
      setReadStatus(prev => ({
        ...prev,
        [userId]: lastReadMessageId
      }));
    };

    socket.on('message:created', onMessageCreated);
    socket.on('message:updated', onMessageUpdated);
    socket.on('message:deleted', onMessageDeleted);
    socket.on('typing:started', onTypingStarted);
    socket.on('typing:stopped', onTypingStopped);
    socket.on('room:read_updated', onReadUpdated);

    return () => {
      socket.emit('room:leave', roomId);
      socket.off('message:created');
      socket.off('message:updated');
      socket.off('message:deleted');
      socket.off('typing:started');
      socket.off('typing:stopped');
      socket.off('room:read_updated');
    };
  }, [roomId, user]);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  const markAsRead = (messageId: number) => {
    const token = localStorage.getItem('token');
    fetch(`/api/rooms/${roomId}/read`, {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}` 
      },
      body: JSON.stringify({ lastReadMessageId: messageId })
    });
  };

  const handleReact = async (id: number, emoji: string) => {
    const token = localStorage.getItem('token');
    await fetch(`/api/messages/${id}/reactions`, {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}` 
      },
      body: JSON.stringify({ emoji })
    });
  };

  const handleEdit = async (id: number, content: string) => {
    const token = localStorage.getItem('token');
    await fetch(`/api/messages/${id}`, {
      method: 'PUT',
      headers: { 
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}` 
      },
      body: JSON.stringify({ content })
    });
  };

  // Compute "Seen by" for each message
  // For a message M, who has read >= M.id?
  const getSeenBy = (messageId: number) => {
    return Object.entries(readStatus)
      .filter(([uid, lastRead]) => lastRead >= messageId && uid !== user?.id) // Exclude self
      .map(([uid]) => {
         // Need username. But I only have IDs in readStatus.
         // I can fetch user list or just rely on finding user in previous messages?
         // For now, I'll display User ID or try to find it in message authors.
         const knownUser = messages.find(m => m.userId === uid)?.author;
         return knownUser ? knownUser.username : uid.slice(0, 8);
      });
  };

  if (loading) return <div style={{ padding: 20 }}>Loading messages...</div>;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ padding: 16, borderBottom: '1px solid var(--bg-tertiary)' }}>
        <h3 style={{ margin: 0 }}>Room #{roomId}</h3>
      </div>
      
      <div 
        ref={scrollRef}
        style={{ flex: 1, overflowY: 'auto', display: 'flex', flexDirection: 'column', padding: '16px 0' }}
      >
        {messages.map(msg => (
          <MessageItem 
            key={msg.id} 
            message={msg} 
            seenBy={getSeenBy(msg.id)}
            onReact={handleReact}
            onEdit={handleEdit}
          />
        ))}
        <div ref={messagesEndRef} />
      </div>

      {typingUsers.size > 0 && (
        <div style={{ padding: '0 16px', fontSize: 12, color: 'var(--text-muted)', fontStyle: 'italic' }}>
          {Array.from(typingUsers).join(', ')} {typingUsers.size === 1 ? 'is' : 'are'} typing...
        </div>
      )}

      <MessageInput roomId={parsedRoomId} />
    </div>
  );
}
