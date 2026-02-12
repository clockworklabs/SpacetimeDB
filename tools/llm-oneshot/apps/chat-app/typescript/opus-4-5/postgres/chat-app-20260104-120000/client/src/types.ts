export interface User {
  id: string;
  displayName: string;
  status: 'online' | 'away' | 'dnd' | 'invisible';
  lastActiveAt: string;
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
  id: number;
  roomId: number;
  userId: string;
  isAdmin: boolean;
  isBanned: boolean;
  lastReadAt: string | null;
  joinedAt: string;
}

export interface Message {
  id: number;
  roomId: number;
  userId: string;
  content: string;
  isEdited: boolean;
  replyToId: number | null;
  scheduledFor: string | null;
  expiresAt: string | null;
  createdAt: string;
}

export interface MessageEdit {
  id: number;
  messageId: number;
  previousContent: string;
  editedAt: string;
}

export interface MessageReaction {
  id: number;
  messageId: number;
  userId: string;
  emoji: string;
  createdAt: string;
}

export interface ReadReceipt {
  id: number;
  messageId: number;
  userId: string;
  readAt: string;
}

export interface RoomInvitation {
  id: number;
  roomId: number;
  invitedUserId: string;
  invitedBy: string;
  status: 'pending' | 'accepted' | 'declined';
  createdAt: string;
}

export interface TypingUser {
  roomId: number;
  userId: string;
  user?: User;
}

export interface MessageWithUser {
  message: Message;
  user: User;
}

export interface MemberWithUser {
  member: RoomMember;
  user: User;
}

export interface InvitationWithDetails {
  invitation: RoomInvitation;
  room: Room;
  inviter: User;
}

export interface ReactionWithUser {
  reaction: MessageReaction;
  user: User;
}

export interface ReceiptWithUser {
  receipt: ReadReceipt;
  user: User;
}
