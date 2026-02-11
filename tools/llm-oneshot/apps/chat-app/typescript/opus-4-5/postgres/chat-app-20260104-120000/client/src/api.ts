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
    request<{ user: any; token: string }>('/auth/register', {
      method: 'POST',
      body: JSON.stringify({ displayName }),
    }),

  // Users
  getMe: () => request<{ user: any }>('/users/me'),
  updateMe: (displayName: string) =>
    request<{ user: any }>('/users/me', {
      method: 'PATCH',
      body: JSON.stringify({ displayName }),
    }),
  updateStatus: (status: string) =>
    request<{ user: any }>('/users/me/status', {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  getUsers: () => request<{ users: any[] }>('/users'),
  searchUsers: (q: string) =>
    request<{ users: any[] }>(`/users/search?q=${encodeURIComponent(q)}`),

  // Rooms
  getRooms: () => request<{ rooms: any[] }>('/rooms'),
  createRoom: (name: string, isPrivate?: boolean) =>
    request<{ room: any }>('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, isPrivate }),
    }),
  createDm: (targetUserId: string) =>
    request<{ room: any }>('/dms', {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    }),
  joinRoom: (roomId: number) =>
    request<{ member: any }>(`/rooms/${roomId}/join`, { method: 'POST' }),
  leaveRoom: (roomId: number) =>
    request<{ success: boolean }>(`/rooms/${roomId}/leave`, { method: 'POST' }),
  getMembers: (roomId: number) =>
    request<{ members: any[] }>(`/rooms/${roomId}/members`),
  inviteUser: (roomId: number, username: string) =>
    request<{ invitation: any }>(`/rooms/${roomId}/invite`, {
      method: 'POST',
      body: JSON.stringify({ username }),
    }),
  kickUser: (roomId: number, targetUserId: string) =>
    request<{ success: boolean }>(`/rooms/${roomId}/kick/${targetUserId}`, {
      method: 'POST',
    }),
  banUser: (roomId: number, targetUserId: string) =>
    request<{ success: boolean }>(`/rooms/${roomId}/ban/${targetUserId}`, {
      method: 'POST',
    }),
  promoteUser: (roomId: number, targetUserId: string) =>
    request<{ success: boolean }>(`/rooms/${roomId}/promote/${targetUserId}`, {
      method: 'POST',
    }),

  // Invitations
  getInvitations: () => request<{ invitations: any[] }>('/invitations'),
  respondToInvitation: (invitationId: number, action: 'accept' | 'decline') =>
    request<{ success: boolean; room?: any }>(
      `/invitations/${invitationId}/${action}`,
      { method: 'POST' }
    ),

  // Messages
  getMessages: (roomId: number) =>
    request<{
      messages: any[];
      reactions: any[];
      receipts: any[];
      replyCounts: any[];
    }>(`/rooms/${roomId}/messages`),
  sendMessage: (
    roomId: number,
    content: string,
    options?: { replyToId?: number; scheduledFor?: string; expiresIn?: number }
  ) =>
    request<{ message: any; user: any }>(`/rooms/${roomId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content, ...options }),
    }),
  editMessage: (messageId: number, content: string) =>
    request<{ message: any }>(`/messages/${messageId}`, {
      method: 'PATCH',
      body: JSON.stringify({ content }),
    }),
  getMessageHistory: (messageId: number) =>
    request<{ edits: any[] }>(`/messages/${messageId}/history`),
  getReplies: (messageId: number) =>
    request<{ replies: any[] }>(`/messages/${messageId}/replies`),
  getScheduled: (roomId: number) =>
    request<{ scheduled: any[] }>(`/rooms/${roomId}/scheduled`),
  cancelScheduled: (messageId: number) =>
    request<{ success: boolean }>(`/messages/${messageId}/scheduled`, {
      method: 'DELETE',
    }),

  // Reactions
  toggleReaction: (messageId: number, emoji: string) =>
    request<{ action: string }>(`/messages/${messageId}/reactions`, {
      method: 'POST',
      body: JSON.stringify({ emoji }),
    }),
  getReactions: (messageId: number) =>
    request<{ reactions: any[] }>(`/messages/${messageId}/reactions`),

  // Read receipts
  markRead: (roomId: number, messageIds: number[]) =>
    request<{ success: boolean }>(`/rooms/${roomId}/read`, {
      method: 'POST',
      body: JSON.stringify({ messageIds }),
    }),
  getUnreadCounts: () =>
    request<{ unreadCounts: Record<number, number> }>('/rooms/unread'),
};
