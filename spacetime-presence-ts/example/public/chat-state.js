export const chatState = {
  userId: null,
  userEmail: '',
  activeServerId: null,
  activeRoomId: null,
  authenticated: false,
  servers: [],
  directory: [],
  serverMembers: [],
  rooms: [],
  users: [],
  members: [],
  messages: [],
  reactions: [],
  attachments: [],
  threads: [],
  threadMessages: [],
  cursors: [],
  presence: [],
  rateLimitStatus: [],
};

export function applyChatData(next) {
  const { userId, userEmail } = chatState;
  Object.assign(chatState, next, { userId, userEmail });
}
