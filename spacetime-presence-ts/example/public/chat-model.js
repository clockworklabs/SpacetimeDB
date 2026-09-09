import { chatState as state } from './chat-state.js';

export function hex(identity) {
  return identity.toHexString();
}

export function activeRoom() {
  return state.rooms.find(room => room.id === state.activeRoomId);
}

export function activeServer() {
  return state.servers.find(server => server.id === state.activeServerId);
}

export function messageById(messageId) {
  return state.messages.find(message => message.id === messageId);
}

export function threadMessageById(messageId) {
  return state.threadMessages.find(message => message.id === messageId);
}

export function roomMembers(roomId) {
  return state.members.filter(member => member.roomId === roomId);
}

export function myMemberships() {
  return state.members
    .filter(member => member.userId === state.userId)
    .map(member => member.roomId);
}

function isOwnIdentity(identity) {
  return Boolean(state.meHex) && hex(identity) === state.meHex;
}

function canModerateActiveRoom() {
  const room = activeRoom();
  const server = activeServer();
  return Boolean(
    state.userId &&
      room &&
      (room.createdByUserId === state.userId ||
        server?.createdByUserId === state.userId)
  );
}

export function canEditMessage(message) {
  return Boolean(message && isOwnIdentity(message.author));
}

export function canDeleteMessage(message) {
  return Boolean(
    message && (isOwnIdentity(message.author) || canModerateActiveRoom())
  );
}

export function userByHex() {
  const users = new Map();
  for (const user of state.users) users.set(hex(user.identity), user);
  return users;
}

export function userByUserId() {
  const users = new Map();
  for (const user of state.users) users.set(user.userId, user);
  return users;
}

export function messageAuthorName(message, users = userByHex()) {
  if (!message) return 'message';
  const authorHex = hex(message.author);
  return users.get(authorHex)?.displayName || authorHex.slice(-6);
}

export function messageSummary(message) {
  if (!message) return 'Original message unavailable';
  const text = (message.content || '').trim();
  if (text) return text.length > 120 ? `${text.slice(0, 117)}...` : text;
  const attachmentCount = state.attachments.filter(
    attachment => attachment.messageId === message.id
  ).length;
  return attachmentCount
    ? `${attachmentCount} attachment${attachmentCount === 1 ? '' : 's'}`
    : 'Empty message';
}

function threadForRoot(rootMessageId) {
  return state.threads.find(thread => thread.rootMessageId === rootMessageId);
}

export function threadMessagesForRoot(rootMessageId) {
  const thread = threadForRoot(rootMessageId);
  if (!thread) return [];
  return state.threadMessages.filter(message => message.threadId === thread.id);
}

export function myServers() {
  if (!state.userId) return [];
  const serverIds = new Set();
  for (const member of state.serverMembers) {
    if (member.userId === state.userId) {
      serverIds.add(member.serverId.toString());
    }
  }
  return state.servers.filter(server => serverIds.has(server.id.toString()));
}

export function roomsInServer(serverId) {
  if (serverId === null || serverId === undefined) return [];
  return state.rooms.filter(room => room.serverId === serverId);
}

export function amServerOwner(serverId) {
  const server = state.servers.find(candidate => candidate.id === serverId);
  return Boolean(server && server.createdByUserId === state.userId);
}

export function latestMessageByRoom() {
  const latest = new Map();
  for (const message of state.messages) {
    const existing = latest.get(message.roomId);
    if (!existing || existing.id < message.id) {
      latest.set(message.roomId, message);
    }
  }
  return latest;
}

export function myReadCursorByRoom() {
  const cursors = new Map();
  for (const cursor of state.cursors) {
    if (hex(cursor.identity) === state.meHex) {
      cursors.set(cursor.roomId, cursor.lastReadMessageId);
    }
  }
  return cursors;
}

export function globalPresenceBySubject() {
  const presence = new Map();
  for (const row of state.presence) {
    if (row.scope === 'chat.global') presence.set(row.subject, row);
  }
  return presence;
}

export function typingForRoom(roomId) {
  const scope = `chat.typing:${roomId.toString()}`;
  return state.presence
    .filter(row => row.scope === scope)
    .map(row => row.subject);
}

export function statusOf(subjectHex, presence) {
  return presence.get(subjectHex)?.status || 'invisible';
}

export function attachmentsByMessage() {
  const attachments = new Map();
  for (const attachment of state.attachments) {
    const existing = attachments.get(attachment.messageId) ?? [];
    existing.push(attachment);
    attachments.set(attachment.messageId, existing);
  }
  for (const list of attachments.values()) {
    list.sort((left, right) => left.ordinal - right.ordinal);
  }
  return attachments;
}
