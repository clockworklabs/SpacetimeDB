import { io, Socket } from 'socket.io-client';
import { api } from './api';

const SOCKET_URL = import.meta.env.VITE_API_URL || 'http://localhost:3001';

let socket: Socket | null = null;

export function getSocket(): Socket | null {
  return socket;
}

export function connectSocket(): Socket {
  const token = api.getToken();
  if (!token) {
    throw new Error('No token available');
  }

  if (socket?.connected) {
    return socket;
  }

  socket = io(SOCKET_URL, {
    auth: { token },
  });

  // Activity tracking for auto-away
  let activityTimeout: ReturnType<typeof setTimeout>;
  
  const sendActivity = () => {
    if (socket?.connected) {
      socket.emit('activity');
    }
    clearTimeout(activityTimeout);
    activityTimeout = setTimeout(sendActivity, 60000);
  };

  document.addEventListener('mousemove', sendActivity);
  document.addEventListener('keypress', sendActivity);
  sendActivity();

  return socket;
}

export function disconnectSocket() {
  if (socket) {
    socket.disconnect();
    socket = null;
  }
}
