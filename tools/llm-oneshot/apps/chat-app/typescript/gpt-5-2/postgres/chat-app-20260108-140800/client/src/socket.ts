import { io, Socket } from 'socket.io-client';
import { API_URL } from './config';

export type RealtimeEvents =
  | 'rooms:changed'
  | 'roomMembers:changed'
  | 'message:created'
  | 'message:updated'
  | 'message:deleted'
  | 'reactions:changed'
  | 'reads:changed'
  | 'presence:changed'
  | 'typing:state'
  | 'scheduled:changed';

export type TypingStatePayload = {
  roomId: number;
  users: { userId: string; displayName: string }[];
};

export function connectSocket(token: string): Socket {
  const socket = io(API_URL, {
    transports: ['websocket'],
    auth: { token },
  });
  return socket;
}
