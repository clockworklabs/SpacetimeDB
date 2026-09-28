import Phaser from 'phaser';
import { cameraSize } from './math';
import type { GameManager } from './GameManager';

export class CameraController {
  private worldSize = 1000;

  constructor(
    private readonly scene: Phaser.Scene,
    private readonly gameManager: GameManager
  ) {}

  setWorldSize(worldSize: number): void {
    this.worldSize = worldSize;
  }

  update(delta: number): void {
    const localPlayer = this.gameManager.localPlayer;
    const center = localPlayer?.centerOfMass() ?? {
      x: this.worldSize / 2,
      y: this.worldSize / 2,
    };
    const camera = this.scene.cameras.main;
    camera.centerOn(center.x, center.y);
    if (!localPlayer || localPlayer.numberOfOwnedCircles === 0) {
      return;
    }
    const targetSize = cameraSize(
      localPlayer.totalMass(),
      localPlayer.numberOfOwnedCircles
    );
    const zoom = this.scene.scale.height / (targetSize * 2);
    camera.setZoom(
      Phaser.Math.Linear(camera.zoom, zoom, Math.min(1, (delta / 1000) * 2))
    );
  }
}
