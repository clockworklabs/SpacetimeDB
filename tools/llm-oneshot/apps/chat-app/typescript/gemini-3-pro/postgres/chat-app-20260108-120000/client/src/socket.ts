import { io, Socket } from 'socket.io-client';

export const socket: Socket = io('/', {
  autoConnect: false,
  auth: cb => {
    cb({ token: localStorage.getItem('token') });
  },
});

// Helper to update auth token
export const updateSocketToken = (token: string) => {
  socket.auth = { token };
  if (socket.connected) {
    socket.disconnect();
    socket.connect();
  } else {
    socket.connect();
  }
};
