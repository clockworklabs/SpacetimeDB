const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:3001';

class ApiClient {
  private token: string | null = null;

  constructor() {
    this.token = localStorage.getItem('token');
  }

  setToken(token: string) {
    this.token = token;
    localStorage.setItem('token', token);
  }

  clearToken() {
    this.token = null;
    localStorage.removeItem('token');
  }

  getToken() {
    return this.token;
  }

  private async request<T>(
    path: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    const res = await fetch(`${API_URL}${path}`, {
      ...options,
      headers: { ...headers, ...options.headers },
    });

    if (!res.ok) {
      const error = await res.json().catch(() => ({ error: 'Request failed' }));
      throw new Error(error.error || 'Request failed');
    }

    return res.json();
  }

  // Auth
  register(displayName: string) {
    return this.request<{ user: any; token: string }>('/api/auth/register', {
      method: 'POST',
      body: JSON.stringify({ displayName }),
    });
  }

  getMe() {
    return this.request<any>('/api/auth/me');
  }

  updateDisplayName(displayName: string) {
    return this.request<any>('/api/auth/displayName', {
      method: 'PUT',
      body: JSON.stringify({ displayName }),
    });
  }

  updateStatus(status: string) {
    return this.request<any>('/api/auth/status', {
      method: 'PUT',
      body: JSON.stringify({ status }),
    });
  }

  // Users
  getUsers() {
    return this.request<any[]>('/api/users');
  }

  getUserByName(displayName: string) {
    return this.request<any>(
      `/api/users/by-name/${encodeURIComponent(displayName)}`
    );
  }

  // Rooms
  getPublicRooms() {
    return this.request<any[]>('/api/rooms');
  }

  getMyRooms() {
    return this.request<any[]>('/api/rooms/my');
  }

  createRoom(name: string, roomType: 'public' | 'private' = 'public') {
    return this.request<any>('/api/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, roomType }),
    });
  }

  joinRoom(roomId: number) {
    return this.request<any>(`/api/rooms/${roomId}/join`, { method: 'POST' });
  }

  leaveRoom(roomId: number) {
    return this.request<any>(`/api/rooms/${roomId}/leave`, { method: 'POST' });
  }

  getRoomMembers(roomId: number) {
    return this.request<any[]>(`/api/rooms/${roomId}/members`);
  }

  kickUser(roomId: number, userId: string) {
    return this.request<any>(`/api/rooms/${roomId}/kick/${userId}`, {
      method: 'POST',
    });
  }

  banUser(roomId: number, userId: string) {
    return this.request<any>(`/api/rooms/${roomId}/ban/${userId}`, {
      method: 'POST',
    });
  }

  promoteUser(roomId: number, userId: string) {
    return this.request<any>(`/api/rooms/${roomId}/promote/${userId}`, {
      method: 'POST',
    });
  }

  inviteUser(roomId: number, username: string) {
    return this.request<any>(`/api/rooms/${roomId}/invite`, {
      method: 'POST',
      body: JSON.stringify({ username }),
    });
  }

  getUnreadCounts() {
    return this.request<Record<number, number>>('/api/rooms/unread');
  }

  // Invites
  getInvites() {
    return this.request<any[]>('/api/invites');
  }

  acceptInvite(inviteId: number) {
    return this.request<any>(`/api/invites/${inviteId}/accept`, {
      method: 'POST',
    });
  }

  declineInvite(inviteId: number) {
    return this.request<any>(`/api/invites/${inviteId}/decline`, {
      method: 'POST',
    });
  }

  // DMs
  createDM(targetUserId: string) {
    return this.request<any>('/api/dm', {
      method: 'POST',
      body: JSON.stringify({ targetUserId }),
    });
  }

  // Messages
  getMessages(roomId: number) {
    return this.request<any[]>(`/api/rooms/${roomId}/messages`);
  }

  sendMessage(
    roomId: number,
    content: string,
    options: {
      parentId?: number;
      scheduledFor?: string;
      expiresInMinutes?: number;
    } = {}
  ) {
    return this.request<any>(`/api/rooms/${roomId}/messages`, {
      method: 'POST',
      body: JSON.stringify({ content, ...options }),
    });
  }

  getScheduledMessages(roomId: number) {
    return this.request<any[]>(`/api/rooms/${roomId}/scheduled`);
  }

  cancelScheduledMessage(messageId: number) {
    return this.request<any>(`/api/messages/${messageId}/scheduled`, {
      method: 'DELETE',
    });
  }

  editMessage(messageId: number, content: string) {
    return this.request<any>(`/api/messages/${messageId}`, {
      method: 'PUT',
      body: JSON.stringify({ content }),
    });
  }

  getEditHistory(messageId: number) {
    return this.request<any[]>(`/api/messages/${messageId}/history`);
  }

  getThread(messageId: number) {
    return this.request<any[]>(`/api/messages/${messageId}/thread`);
  }

  // Reactions
  toggleReaction(messageId: number, emoji: string) {
    return this.request<any>(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      body: JSON.stringify({ emoji }),
    });
  }

  getReactions(messageId: number) {
    return this.request<any[]>(`/api/messages/${messageId}/reactions`);
  }

  // Read receipts
  markAsRead(messageId: number) {
    return this.request<any>(`/api/messages/${messageId}/read`, {
      method: 'POST',
    });
  }

  getReadReceipts(messageId: number) {
    return this.request<any[]>(`/api/messages/${messageId}/read`);
  }
}

export const api = new ApiClient();
