export const chatState = {
  meHex: '',
  userId: null,
  userEmail: '',
  activeServerId: null,
  activeRoomId: null,
  authenticated: false,
  admins: [],
  servers: [],
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
