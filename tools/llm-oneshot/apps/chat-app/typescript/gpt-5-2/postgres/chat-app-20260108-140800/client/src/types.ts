export type User = { id: string; displayName: string };

export type Room = {
  id: number;
  name: string;
  lastReadMessageId: number | null;
};

export type Member = {
  id: string;
  displayName: string;
  isOnline: boolean;
  lastReadMessageId: number | null;
};

export type MessageReaction = { emoji: string; userIds: string[] };

export type Message = {
  id: number;
  roomId: number;
  authorId: string;
  authorName: string;
  content: string;
  createdAt: string;
  updatedAt: string | null;
  expiresAt: string | null;
  edited: boolean;
  editCount: number;
  reactions: MessageReaction[];
};

export type ScheduledMessage = {
  id: number;
  roomId: number;
  roomName: string;
  content: string;
  sendAt: string;
};

