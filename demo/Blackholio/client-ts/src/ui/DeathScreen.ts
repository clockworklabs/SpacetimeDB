export class DeathScreen {
  private readonly overlay = document.querySelector(
    '#death-overlay'
  ) as HTMLElement;
  private readonly respawnButton = document.querySelector(
    '#respawn-button'
  ) as HTMLButtonElement;

  constructor(onRespawn: () => void) {
    this.respawnButton.addEventListener('click', onRespawn);
  }

  setVisible(visible: boolean): void {
    this.overlay.classList.toggle('hidden', !visible);
  }
}
