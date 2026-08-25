function encodePart(value: string): string {
  return `${value.length}:${value}`;
}

export function buildRateLimitKey(scope: string, actorKey: string): string {
  return `${encodePart(scope)}${encodePart(actorKey)}`;
}
