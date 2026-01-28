export interface User {
  id: string;
  displayName: string;
}

export interface Room {
  id: string;
  name: string;
  createdAt: Date;
  memberCount: number;
}

export interface Message {
  id: string;
  roomId: string;
  userId: string;
  displayName: string;
  content: string;
  createdAt: Date;
  updatedAt: Date;
  isDeleted?: boolean;
  expiresAt?: Date;
  reactions?: Reaction[];
}

export interface Reaction {
  emoji: string;
  count: number;
  users: string[];
}

export interface OnlineUser {
  userId: string;
  displayName: string;
}

export interface UnreadCount {
  roomId: string;
  count: number;
}

export interface TypingUser {
  userId: string;
  displayName: string;
}