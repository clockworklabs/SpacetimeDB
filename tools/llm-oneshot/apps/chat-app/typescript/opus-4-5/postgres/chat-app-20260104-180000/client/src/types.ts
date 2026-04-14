export interface User {
  id: string;
  displayName: string;
  status: 'online' | 'away' | 'dnd' | 'invisible' | 'offline';
  lastActiveAt: string;
  createdAt: string;
  role?: 'member' | 'admin';
}

export interface Room {
  id: number;
  name: string;
  createdBy: string | null;
  roomType: 'public' | 'private' | 'dm';
  createdAt: string;
  role?: 'member' | 'admin';
  lastReadAt?: string;
}

export interface Message {
  id: number;
  roomId: number;
  userId: string;
  content: string;
  parentId: number | null;
  isEdited: boolean;
  createdAt: string;
  scheduledFor: string | null;
  isScheduled: boolean;
  expiresAt: string | null;
  user: User;
}

export interface MessageEdit {
  id: number;
  messageId: number;
  previousContent: string;
  editedAt: string;
}

export interface Reaction {
  emoji: string;
  count: number;
  users: string[];
}

export interface RoomInvite {
  id: number;
  room: Room;
  invitedBy: User;
  createdAt: string;
}

export interface TypingUser {
  userId: string;
  displayName: string;
}
