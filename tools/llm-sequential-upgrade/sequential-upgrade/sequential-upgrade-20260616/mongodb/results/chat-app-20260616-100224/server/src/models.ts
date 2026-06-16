import mongoose, { Schema, Document } from 'mongoose';

export interface IUser extends Document {
  name: string;
  online: boolean;
  socketId?: string;
  lastSeen: Date;
}

const UserSchema = new Schema<IUser>({
  name: { type: String, required: true, unique: true, trim: true, maxlength: 32 },
  online: { type: Boolean, default: false },
  socketId: { type: String },
  lastSeen: { type: Date, default: Date.now },
});

export const User = mongoose.model<IUser>('User', UserSchema);

export interface IRoom extends Document {
  name: string;
  createdBy: string;
  members: string[];
  createdAt: Date;
}

const RoomSchema = new Schema<IRoom>({
  name: { type: String, required: true, unique: true, trim: true, maxlength: 64 },
  createdBy: { type: String, required: true },
  members: [{ type: String }],
  createdAt: { type: Date, default: Date.now },
});

export const Room = mongoose.model<IRoom>('Room', RoomSchema);

export interface IMessage extends Document {
  roomId: mongoose.Types.ObjectId;
  sender: string;
  text: string;
  createdAt: Date;
  readBy: string[];
}

const MessageSchema = new Schema<IMessage>({
  roomId: { type: Schema.Types.ObjectId, ref: 'Room', required: true },
  sender: { type: String, required: true },
  text: { type: String, required: true, maxlength: 2000 },
  createdAt: { type: Date, default: Date.now },
  readBy: [{ type: String }],
});

MessageSchema.index({ roomId: 1, createdAt: 1 });

export const Message = mongoose.model<IMessage>('Message', MessageSchema);
