import React, { useState, useEffect } from 'react';
import { io, Socket } from 'socket.io-client';
import Login from './components/Login';
import Chat from './components/Chat';
import { User, Room, Message, OnlineUser, UnreadCount } from './types';

function App() {
  const [socket, setSocket] = useState<Socket | null>(null);
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [onlineUsers, setOnlineUsers] = useState<OnlineUser[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<UnreadCount[]>([]);
  const [typingUsers, setTypingUsers] = useState<{
    [roomId: string]: { userId: string; displayName: string }[];
  }>({});

  useEffect(() => {
    const newSocket = io('http://localhost:3001');
    setSocket(newSocket);

    return () => {
      newSocket.close();
    };
  }, []);

  useEffect(() => {
    if (!socket) return;

    socket.on(
      'authenticated',
      (data: { userId: string; displayName: string }) => {
        setCurrentUser({ id: data.userId, displayName: data.displayName });
        setIsAuthenticated(true);
        // Request all available rooms after authentication
        socket.emit('get_all_rooms');
      }
    );

    socket.on(
      'initial_data',
      (data: {
        rooms: Room[];
        onlineUsers: OnlineUser[];
        unreadCounts: UnreadCount[];
      }) => {
        setRooms(data.rooms);
        setOnlineUsers(data.onlineUsers);
        setUnreadCounts(data.unreadCounts);
      }
    );

    socket.on('all_rooms', (data: { rooms: Room[] }) => {
      setRooms(data.rooms);
    });

    socket.on('room_created', (room: Room) => {
      // Add new room if not already in list
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) {
          return prev;
        }
        return [...prev, room];
      });
    });

    socket.on(
      'message_deleted',
      (data: { messageId: string; roomId: string }) => {
        // Remove deleted message from the list
        setMessages(prev => prev.filter(msg => msg.id !== data.messageId));
      }
    );

    socket.on(
      'room_joined',
      (data: { roomId: string; messages: Message[] }) => {
        setCurrentRoomId(data.roomId);
        setMessages(data.messages);
      }
    );

    socket.on('new_message', (message: Message) => {
      setMessages(prev => [...prev, message]);
    });

    socket.on(
      'message_edited',
      (data: { messageId: string; content: string; updatedAt: Date }) => {
        setMessages(prev =>
          prev.map(msg =>
            msg.id === data.messageId
              ? { ...msg, content: data.content, updatedAt: data.updatedAt }
              : msg
          )
        );
      }
    );

    socket.on('user_online', (user: OnlineUser) => {
      setOnlineUsers(prev => [
        ...prev.filter(u => u.userId !== user.userId),
        user,
      ]);
    });

    socket.on('user_offline', (data: { userId: string }) => {
      setOnlineUsers(prev => prev.filter(u => u.userId !== data.userId));
    });

    socket.on(
      'user_typing',
      (data: { userId: string; displayName: string; roomId: string }) => {
        setTypingUsers(prev => ({
          ...prev,
          [data.roomId]: [
            ...(prev[data.roomId] || []).filter(u => u.userId !== data.userId),
            { userId: data.userId, displayName: data.displayName },
          ],
        }));
      }
    );

    socket.on(
      'user_stopped_typing',
      (data: { userId: string; roomId: string }) => {
        setTypingUsers(prev => ({
          ...prev,
          [data.roomId]: (prev[data.roomId] || []).filter(
            u => u.userId !== data.userId
          ),
        }));
      }
    );

    socket.on(
      'message_read',
      (data: { messageId: string; userId: string; displayName: string }) => {
        // Could update read receipts display here
      }
    );

    socket.on(
      'reaction_updated',
      (data: { messageId: string; reactions: any[] }) => {
        setMessages(prev =>
          prev.map(msg =>
            msg.id === data.messageId
              ? { ...msg, reactions: data.reactions }
              : msg
          )
        );
      }
    );

    socket.on(
      'unread_count_updated',
      (data: { roomId: string; count: number }) => {
        setUnreadCounts(prev =>
          prev.map(uc =>
            uc.roomId === data.roomId ? { ...uc, count: data.count } : uc
          )
        );
      }
    );

    socket.on('error', (error: { message: string }) => {
      alert(error.message);
    });

    return () => {
      socket.off('authenticated');
      socket.off('initial_data');
      socket.off('all_rooms');
      socket.off('room_created');
      socket.off('room_joined');
      socket.off('new_message');
      socket.off('message_edited');
      socket.off('message_deleted');
      socket.off('user_online');
      socket.off('user_offline');
      socket.off('user_typing');
      socket.off('user_stopped_typing');
      socket.off('message_read');
      socket.off('reaction_updated');
      socket.off('unread_count_updated');
      socket.off('error');
    };
  }, [socket]);

  const authenticate = (displayName: string) => {
    if (socket) {
      socket.emit('authenticate', { displayName });
    }
  };

  const createRoom = (name: string) => {
    if (socket) {
      socket.emit('create_room', { name });
    }
  };

  const joinRoom = (roomId: string) => {
    if (socket) {
      socket.emit('join_room', { roomId });
    }
  };

  const sendMessage = (
    roomId: string,
    content: string,
    scheduledFor?: Date,
    expiresAt?: Date
  ) => {
    if (socket) {
      socket.emit('send_message', { roomId, content, scheduledFor, expiresAt });
    }
  };

  const editMessage = (messageId: string, content: string) => {
    if (socket) {
      socket.emit('edit_message', { messageId, content });
    }
  };

  const startTyping = (roomId: string) => {
    if (socket) {
      socket.emit('start_typing', { roomId });
    }
  };

  const stopTyping = (roomId: string) => {
    if (socket) {
      socket.emit('stop_typing', { roomId });
    }
  };

  const markAsRead = (roomId: string, messageId: string) => {
    if (socket) {
      socket.emit('mark_as_read', { roomId, messageId });
    }
  };

  const addReaction = (messageId: string, emoji: string) => {
    if (socket) {
      socket.emit('add_reaction', { messageId, emoji });
    }
  };

  const removeReaction = (messageId: string, emoji: string) => {
    if (socket) {
      socket.emit('remove_reaction', { messageId, emoji });
    }
  };

  if (!isAuthenticated) {
    return <Login onAuthenticate={authenticate} />;
  }

  return (
    <Chat
      currentUser={currentUser!}
      rooms={rooms}
      currentRoomId={currentRoomId}
      messages={messages}
      onlineUsers={onlineUsers}
      unreadCounts={unreadCounts}
      typingUsers={typingUsers}
      onCreateRoom={createRoom}
      onJoinRoom={joinRoom}
      onSendMessage={sendMessage}
      onEditMessage={editMessage}
      onStartTyping={startTyping}
      onStopTyping={stopTyping}
      onMarkAsRead={markAsRead}
      onAddReaction={addReaction}
      onRemoveReaction={removeReaction}
    />
  );
}

export default App;
