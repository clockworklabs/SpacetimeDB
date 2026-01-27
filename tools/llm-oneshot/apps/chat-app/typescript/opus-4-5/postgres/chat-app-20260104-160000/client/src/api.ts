const API_URL = 'http://localhost:3001/api';

function getToken(): string | null {
  return localStorage.getItem('token');
}

async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const token = getToken();
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
    ...options.headers,
  };

  const response = await fetch(`${API_URL}${endpoint}`, {
    ...options,
    headers,
  });

  if (!response.ok) {
    const error = await response
      .json()
      .catch(() => ({ error: 'Request failed' }));
    throw new Error(error.error || 'Request failed');
  }

  return response.json();
}

export const api = {
  // Auth
  register: (displayName: string) =>
    request<{ token: string; user: any }>('/auth/register', {
      method: 'POST',
      body: JSON.stringify({ displayName }),
    }),

  getMe: () => request<any>('/users/me'),

  updateStatus: (status: string) =>
    request<any>('/users/status', {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),

  searchUsers: (q: string) =>
    request<any[]>(`/users/search?q=${encodeURIComponent(q)}`),

  getOnlineUsers: () => request<any[]>('/users/online'),

  // Rooms
  getRooms: () => request<any[]>('/rooms'),

  createRoom: (name: string, isPrivate: boolean) =>
    request<any>('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, isPrivate }),
    }),

  createDM: (targetUserId: string) =>
    request<any>('/rooms/dm', {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),

  joinRoom: (roomId: number) =>
    request<any>(`/rooms/${roomId}/join`, { method: 'POST' }),

  leaveRoom: (roomId: number) =>
    request<any>(`/rooms/${roomId}/leave`, { method: 'POST' }),

  getRoomMembers: (roomId: number) =>
    request<any[]>(`/rooms/${roomId}/members`),

  inviteToRoom: (roomId: number, targetUserId: string) =>
    request<any>(`/rooms/${roomId}/invite`, {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),

  kickFromRoom: (roomId: number, targetUserId: string) =>
    request<any>(`/rooms/${roomId}/kick`, {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),

  banFromRoom: (roomId: number, targetUserId: string) =>
    request<any>(`/rooms/${roomId}/ban`, {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),

  promoteInRoom: (roomId: number, targetUserId: string) =>
    request<any>(`/rooms/${roomId}/promote`, {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),

  // Invitations
  getInvitations: () => request<any[]>('/invitations'),

  respondToInvitation: (id: number, accept: boolean) =>
    request<any>(`/invitations/${id}/respond`, {
      method: 'POST',
      body: JSON.stringify({ accept }),
    }),

  // Messages
  getMessages: (roomId: number) => request<any[]>(`/rooms/${roomId}/messages`),

  getScheduledMessages: (roomId: number) =>
    request<any[]>(`/rooms/${roomId}/scheduled`),

  sendMessage: (
    roomId: number,
    content: string,
    options?: {
      parentMessageId?: number;
      scheduledFor?: string;
      ephemeralMinutes?: number;
    }
  ) =>
    request<any>(`/rooms/${roomId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content, ...options }),
    }),

  cancelScheduledMessage: (messageId: number) =>
    request<any>(`/messages/${messageId}/scheduled`, { method: 'DELETE' }),

  editMessage: (messageId: number, content: string) =>
    request<any>(`/messages/${messageId}`, {
      method: 'PATCH',
      body: JSON.stringify({ content }),
    }),

  getEditHistory: (messageId: number) =>
    request<any[]>(`/messages/${messageId}/history`),

  getThreadReplies: (messageId: number) =>
    request<any[]>(`/messages/${messageId}/replies`),

  toggleReaction: (messageId: number, emoji: string) =>
    request<any>(`/messages/${messageId}/reactions`, {
      method: 'POST',
      body: JSON.stringify({ emoji }),
    }),

  // Read receipts
  markAsRead: (roomId: number, messageIds: number[]) =>
    request<any>(`/rooms/${roomId}/read`, {
      method: 'POST',
      body: JSON.stringify({ messageIds }),
    }),

  getReadReceipts: (messageId: number) =>
    request<any[]>(`/messages/${messageId}/receipts`),

  getUnreadCounts: () => request<Record<number, number>>('/unread'),
};
