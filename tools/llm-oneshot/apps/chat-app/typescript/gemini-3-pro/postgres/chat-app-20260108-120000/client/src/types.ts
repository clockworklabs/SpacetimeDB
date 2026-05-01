export interface User {
  id: string;
  username: string;
  createdAt: string;
}

export interface Room {
  id: number;
  name: string;
  unreadCount?: number;
}

export interface Reaction {
  id: number;
  userId: string;
  emoji: string;
  user?: User;
}

export interface MessageEdit {
  id: number;
  content: string;
  editedAt: string;
}

export interface Message {
  id: number;
  roomId: number;
  userId: string;
  content: string;
  createdAt: string;
  scheduledFor?: string | null;
  expiresAt?: string | null;
  editedAt?: string | null;
  author: User;
  reactions: Reaction[];
  edits: MessageEdit[];
}
