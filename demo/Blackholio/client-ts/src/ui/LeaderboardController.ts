import type { PlayerController } from '../game/PlayerController';
import { leaderboardRows } from '../game/leaderboard';

export class LeaderboardController {
  private readonly root = document.querySelector('#leaderboard') as HTMLElement;
  private readonly list = this.root.querySelector('ol') as HTMLOListElement;

  update(
    players: Iterable<PlayerController>,
    localPlayer?: PlayerController
  ): void {
    const rows = leaderboardRows(
      Array.from(players, player => ({
        id: player.playerId,
        name: player.username,
        mass: player.totalMass(),
        local: player === localPlayer,
      }))
    );
    this.root.classList.toggle('hidden', rows.length === 0);
    this.list.replaceChildren(
      ...rows.map(player => {
        const item = document.createElement('li');
        item.classList.toggle('local', player.local);
        item.textContent = `${player.name} - ${player.mass}`;
        return item;
      })
    );
  }
}
