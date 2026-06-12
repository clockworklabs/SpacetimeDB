export type PlayerScore = {
  id: number;
  name: string;
  mass: number;
  local: boolean;
};

export function leaderboardRows(
  players: readonly PlayerScore[],
  limit = 10
): PlayerScore[] {
  const live = players.filter(player => player.mass > 0);
  const leaders = [...live]
    .sort((a, b) => b.mass - a.mass)
    .slice(0, limit);
  const local = live.find(player => player.local);
  if (local && !leaders.some(player => player.id === local.id)) {
    leaders.push(local);
  }
  return leaders;
}
