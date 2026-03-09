export interface User {
  id: string;
  displayName: string;
  status: 'online' | 'away' | 'dnd' | 'invisible';
  lastActive: string;
  createdAt: string;
}

export interface Room {
  id: number;
  name: string;
  isPrivate: boolean;
  isDm: boolean;
  createdBy: string;
  createdAt: string;
}

export interface RoomMember {
  member: {
    id: number;
    roomId: number;
    userId: string;
    role: 'admin' | 'member';
    isBanned: boolean;
    lastReadAt: string | null;
    joinedAt: string;
  };
  user: User;
}

export interface Reaction {
  id: number;
  messageId: number;
  userId: string;
  emoji: string;
  createdAt: string;
  user?: User;
}

export interface Message {
  id: number;
  roomId: number;
  userId: string;
  content: string;
  isEdited: boolean;
  parentMessageId: number | null;
  isScheduled: boolean;
  scheduledFor: string | null;
  isEphemeral: boolean;
  expiresAt: string | null;
  createdAt: string;
  user: User;
  reactions: Reaction[];
  replyCount: number;
}

export interface MessageEdit {
  id: number;
  messageId: number;
  previousContent: string;
  editedAt: string;
}

export interface ReadReceipt {
  id: number;
  messageId: number;
  userId: string;
  readAt: string;
  user: User;
}

export interface RoomInvitation {
  invitation: {
    id: number;
    roomId: number;
    invitedUserId: string;
    invitedBy: string;
    status: 'pending' | 'accepted' | 'declined';
    createdAt: string;
  };
  room: Room;
}

export interface TypingUser {
  roomId: number;
  userId: string;
}
