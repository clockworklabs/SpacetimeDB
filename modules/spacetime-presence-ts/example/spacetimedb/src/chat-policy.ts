import { client } from '@spacetimedb/rate-limit/submodule';

export const PRESENCE_SCOPE_GLOBAL = 'chat.global';
export const PRESENCE_SCOPE_TYPING_PREFIX = 'chat.typing:';

export const RATE_LIMIT_SEND = client({
  scope: 'chat.send_message',
  limit: 20,
  windowSeconds: 30,
});
export const RATE_LIMIT_TYPING = client({
  scope: 'chat.typing',
  limit: 40,
  windowSeconds: 10,
});
export const RATE_LIMIT_ROOM_WRITE = client({
  scope: 'chat.room_write',
  limit: 10,
  windowSeconds: 60,
});
export const RATE_LIMIT_REACTION = client({
  scope: 'chat.reaction',
  limit: 40,
  windowSeconds: 60,
});
export const RATE_LIMIT_PROFILE = client({
  scope: 'chat.profile',
  limit: 20,
  windowSeconds: 60,
});

export function typingScope(roomId: bigint): string {
  return `${PRESENCE_SCOPE_TYPING_PREFIX}${roomId}`;
}
