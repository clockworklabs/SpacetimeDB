import mongoose, { Schema, Document } from 'mongoose';

export interface IUser extends Document {
  name: string;
  online: boolean;
  socketId?: string;
  lastSeen: Date;
  status: 'online' | 'away' | 'dnd' | 'invisible';
}

const UserSchema = new Schema<IUser>({
  name: { type: String, required: true, unique: true, trim: true, maxlength: 32 },
  online: { type: Boolean, default: false },
  socketId: { type: String },
  lastSeen: { type: Date, default: Date.now },
  status: { type: String, enum: ['online', 'away', 'dnd', 'invisible'], default: 'online' },
});

export const User = mongoose.model<IUser>('User', UserSchema);

export interface IRoom extends Document {
  name: string;
  createdBy: string;
  members: string[];
  admins: string[];
  banned: string[];
  isPrivate: boolean;
  isDM: boolean;
  dmUsers: string[];
  createdAt: Date;
}

const RoomSchema = new Schema<IRoom>({
  name: { type: String, required: true, unique: true, trim: true, maxlength: 128 },
  createdBy: { type: String, required: true },
  members: [{ type: String }],
  admins: [{ type: String }],
  banned: [{ type: String }],
  isPrivate: { type: Boolean, default: false },
  isDM: { type: Boolean, default: false },
  dmUsers: [{ type: String }],
  createdAt: { type: Date, default: Date.now },
});

export const Room = mongoose.model<IRoom>('Room', RoomSchema);

export interface IReaction {
  emoji: string;
  users: string[];
}

export interface IEditEntry {
  text: string;
  editedAt: Date;
}

export interface IMessage extends Document {
  roomId: mongoose.Types.ObjectId;
  sender: string;
  text: string;
  createdAt: Date;
  readBy: string[];
  expiresAt?: Date;
  reactions: IReaction[];
  editHistory: IEditEntry[];
  isEdited: boolean;
  parentId?: mongoose.Types.ObjectId;
  replyCount: number;
  lastReplyPreview?: string;
  lastReplySender?: string;
}

const MessageSchema = new Schema<IMessage>({
  roomId: { type: Schema.Types.ObjectId, ref: 'Room', required: true },
  sender: { type: String, required: true },
  text: { type: String, required: true, maxlength: 2000 },
  createdAt: { type: Date, default: Date.now },
  readBy: [{ type: String }],
  expiresAt: { type: Date, default: null },
  reactions: [{ emoji: { type: String, required: true }, users: [{ type: String }] }],
  editHistory: [{ text: { type: String, required: true }, editedAt: { type: Date, required: true } }],
  isEdited: { type: Boolean, default: false },
  parentId: { type: Schema.Types.ObjectId, ref: 'Message', default: null },
  replyCount: { type: Number, default: 0 },
  lastReplyPreview: { type: String, default: null },
  lastReplySender: { type: String, default: null },
});

MessageSchema.index({ roomId: 1, createdAt: 1 });
MessageSchema.index({ expiresAt: 1 }, { sparse: true });
MessageSchema.index({ parentId: 1, createdAt: 1 });

export const Message = mongoose.model<IMessage>('Message', MessageSchema);

export interface IScheduledMessage extends Document {
  roomId: mongoose.Types.ObjectId;
  sender: string;
  text: string;
  scheduledAt: Date;
  sent: boolean;
  createdAt: Date;
}

const ScheduledMessageSchema = new Schema<IScheduledMessage>({
  roomId: { type: Schema.Types.ObjectId, ref: 'Room', required: true },
  sender: { type: String, required: true },
  text: { type: String, required: true, maxlength: 2000 },
  scheduledAt: { type: Date, required: true },
  sent: { type: Boolean, default: false },
  createdAt: { type: Date, default: Date.now },
});

ScheduledMessageSchema.index({ scheduledAt: 1, sent: 1 });

export const ScheduledMessage = mongoose.model<IScheduledMessage>('ScheduledMessage', ScheduledMessageSchema);

export interface IInvitation extends Document {
  roomId: mongoose.Types.ObjectId;
  roomName: string;
  invitedUser: string;
  invitedBy: string;
  status: 'pending' | 'accepted' | 'declined';
  createdAt: Date;
}

const InvitationSchema = new Schema<IInvitation>({
  roomId: { type: Schema.Types.ObjectId, ref: 'Room', required: true },
  roomName: { type: String, required: true },
  invitedUser: { type: String, required: true },
  invitedBy: { type: String, required: true },
  status: { type: String, enum: ['pending', 'accepted', 'declined'], default: 'pending' },
  createdAt: { type: Date, default: Date.now },
});

InvitationSchema.index({ invitedUser: 1, status: 1 });

export const Invitation = mongoose.model<IInvitation>('Invitation', InvitationSchema);

export interface IDraft extends Document {
  userName: string;
  roomId: string;
  text: string;
  updatedAt: Date;
}

const DraftSchema = new Schema<IDraft>({
  userName: { type: String, required: true },
  roomId: { type: String, required: true },
  text: { type: String, required: true, maxlength: 2000 },
  updatedAt: { type: Date, default: Date.now },
});

DraftSchema.index({ userName: 1, roomId: 1 }, { unique: true });

export const Draft = mongoose.model<IDraft>('Draft', DraftSchema);
