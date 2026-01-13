import { API_URL } from './config';
import type { Member, Message, Room, ScheduledMessage, User } from './types';

function getToken(): string | null {
  return localStorage.getItem('chat_token');
}

function setToken(token: string) {
  localStorage.setItem('chat_token', token);
}

export function clearToken() {
  localStorage.removeItem('chat_token');
}

async function http<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken();
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(init?.headers || {}),
    },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(text || `HTTP ${res.status}`);
  }
  return (await res.json()) as T;
}

export async function login(displayName: string): Promise<User> {
  const data = await http<{ token: string; user: User }>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ displayName }),
  });
  setToken(data.token);
  return data.user;
}

export async function getMe(): Promise<User | null> {
  const token = getToken();
  if (!token) return null;
  try {
    const data = await http<{ user: User }>('/me');
    return data.user;
  } catch {
    clearToken();
    return null;
  }
}

export async function setDisplayName(displayName: string): Promise<void> {
  await http<{ ok: true }>('/me', { method: 'PATCH', body: JSON.stringify({ displayName }) });
}

export async function listRooms(): Promise<Room[]> {
  const data = await http<{ rooms: Room[] }>('/rooms');
  return data.rooms;
}

export async function createRoom(name: string): Promise<Room> {
  const data = await http<{ room: { id: number; name: string } }>('/rooms', {
    method: 'POST',
    body: JSON.stringify({ name }),
  });
  return { id: data.room.id, name: data.room.name, lastReadMessageId: null };
}

export async function joinRoom(roomId: number): Promise<void> {
  await http<{ ok: true }>(`/rooms/${roomId}/join`, { method: 'POST', body: JSON.stringify({}) });
}

export async function leaveRoom(roomId: number): Promise<void> {
  await http<{ ok: true }>(`/rooms/${roomId}/leave`, { method: 'POST', body: JSON.stringify({}) });
}

export async function listMembers(roomId: number): Promise<Member[]> {
  const data = await http<{ members: Member[] }>(`/rooms/${roomId}/members`);
  return data.members;
}

export async function listMessages(roomId: number): Promise<Message[]> {
  const data = await http<{ messages: Message[] }>(`/rooms/${roomId}/messages`);
  return data.messages;
}

export async function sendMessage(roomId: number, content: string): Promise<void> {
  await http(`/rooms/${roomId}/messages`, { method: 'POST', body: JSON.stringify({ content }) });
}

export async function sendEphemeral(roomId: number, content: string, seconds: number): Promise<void> {
  await http(`/rooms/${roomId}/messages`, {
    method: 'POST',
    body: JSON.stringify({ content, ephemeralSeconds: seconds }),
  });
}

export async function scheduleMessage(roomId: number, content: string, scheduleInSeconds: number): Promise<void> {
  await http(`/rooms/${roomId}/messages`, {
    method: 'POST',
    body: JSON.stringify({ content, scheduleInSeconds }),
  });
}

export async function markRead(roomId: number, lastReadMessageId: number | null): Promise<void> {
  await http(`/rooms/${roomId}/read`, {
    method: 'POST',
    body: JSON.stringify({ lastReadMessageId }),
  });
}

export async function editMessage(messageId: number, content: string): Promise<void> {
  await http(`/messages/${messageId}`, { method: 'PATCH', body: JSON.stringify({ content }) });
}

export async function getEditHistory(messageId: number): Promise<{ content: string; label: string }[]> {
  const data = await http<{ versions: { content: string; label: string }[] }>(
    `/messages/${messageId}/history`,
  );
  return data.versions;
}

export async function toggleReaction(messageId: number, emoji: string): Promise<void> {
  await http(`/messages/${messageId}/reactions`, {
    method: 'POST',
    body: JSON.stringify({ emoji }),
  });
}

export async function listScheduled(): Promise<ScheduledMessage[]> {
  const data = await http<{ scheduled: ScheduledMessage[] }>('/scheduled');
  return data.scheduled;
}

export async function cancelScheduled(id: number): Promise<void> {
  await http(`/scheduled/${id}`, { method: 'DELETE' });
}

export function getStoredToken(): string | null {
  return getToken();
}

