import type { PlayerController } from '../game/PlayerController';

export class StatusHud {
  private readonly status = document.querySelector('#status') as HTMLElement;
  private readonly mass = document.querySelector('#mass') as HTMLElement;
  private readonly massValue = this.mass.querySelector('strong') as HTMLElement;

  setStatus(message: string, state: 'pending' | 'connected' | 'error'): void {
    this.status.textContent = message;
    this.status.className = state === 'pending' ? '' : state;
  }

  update(localPlayer?: PlayerController): void {
    this.mass.classList.toggle('hidden', !localPlayer);
    this.massValue.textContent = String(localPlayer?.totalMass() ?? 0);
  }
}
