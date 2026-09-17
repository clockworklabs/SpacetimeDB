export function lobbyCompositeKey(...parts: string[]): string {
  return parts.map(part => `${part.length}:${part}`).join('');
}
