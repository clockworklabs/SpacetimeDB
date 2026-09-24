import assert from 'node:assert/strict';
import {
  activeRoom,
  activeServer,
  amServerOwner,
  attachmentsByMessage,
  canDeleteMessage,
  canEditMessage,
  globalPresenceBySubject,
  latestMessageByRoom,
  messageAuthorName,
  messageSummary,
  myMemberships,
  myReadCursorByRoom,
  myServers,
  roomsInServer,
  threadMessagesForRoot,
  typingForRoom,
  userByHex,
} from '../public/chat-model.js';
import { applyChatData, chatState } from '../public/chat-state.js';

const identity = value => ({ toHexString: () => value });
const me = identity('me');
const other = identity('other');

Object.assign(chatState, {
  meHex: 'me',
  userId: 10n,
  userEmail: 'me@example.com',
  activeServerId: 1n,
  activeRoomId: 2n,
  servers: [{ id: 1n, name: 'Server', createdByUserId: 10n }],
  serverMembers: [
    { serverId: 1n, userId: 10n },
    { serverId: 3n, userId: 20n },
  ],
  rooms: [
    { id: 2n, serverId: 1n, createdByUserId: 20n },
    { id: 4n, serverId: 3n, createdByUserId: 20n },
  ],
  users: [
    { userId: 10n, identity: me, displayName: 'Me' },
    { userId: 20n, identity: other, displayName: 'Other' },
  ],
  members: [
    { roomId: 2n, userId: 10n },
    { roomId: 4n, userId: 20n },
  ],
  messages: [
    { id: 5n, roomId: 2n, author: other, content: 'First' },
    { id: 8n, roomId: 2n, author: me, content: '' },
  ],
  attachments: [
    { id: 2n, fileId: 2n, messageId: 8n, ordinal: 2 },
    { id: 1n, fileId: 1n, messageId: 8n, ordinal: 1 },
  ],
  threads: [{ id: 7n, rootMessageId: 5n }],
  threadMessages: [
    { id: 9n, threadId: 7n },
    { id: 10n, threadId: 8n },
  ],
  cursors: [
    { identity: me, roomId: 2n, lastReadMessageId: 5n },
    { identity: other, roomId: 2n, lastReadMessageId: 8n },
  ],
  presence: [
    { scope: 'chat.global', subject: 'me', status: 'online' },
    { scope: 'chat.typing:2', subject: 'other', status: 'online' },
  ],
});

assert.equal(activeServer()?.id, 1n);
assert.equal(activeRoom()?.id, 2n);
assert.equal(amServerOwner(1n), true);
assert.deepEqual(
  myServers().map(server => server.id),
  [1n]
);
assert.deepEqual(
  roomsInServer(1n).map(room => room.id),
  [2n]
);
assert.deepEqual(myMemberships(), [2n]);
assert.equal(canEditMessage(chatState.messages[1]), true);
assert.equal(canEditMessage(chatState.messages[0]), false);
assert.equal(canDeleteMessage(chatState.messages[0]), true);
assert.equal(messageAuthorName(chatState.messages[0]), 'Other');
assert.equal(messageSummary(chatState.messages[1]), '2 attachments');
assert.deepEqual(
  threadMessagesForRoot(5n).map(message => message.id),
  [9n]
);
assert.equal(latestMessageByRoom().get(2n)?.id, 8n);
assert.equal(myReadCursorByRoom().get(2n), 5n);
assert.equal(globalPresenceBySubject().get('me')?.status, 'online');
assert.deepEqual(typingForRoom(2n), ['other']);
assert.equal(userByHex().get('me')?.displayName, 'Me');
assert.deepEqual(
  attachmentsByMessage()
    .get(8n)
    ?.map(attachment => attachment.id),
  [1n, 2n]
);

applyChatData({ activeServerId: 3n, activeRoomId: 4n, rooms: [] });
assert.equal(chatState.activeServerId, 3n);
assert.equal(chatState.userId, 10n);
assert.equal(chatState.userEmail, 'me@example.com');

console.log('presence UI model tests passed');
