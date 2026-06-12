import Phaser from 'phaser';
import { CameraController } from './CameraController';
import { GameManager } from './GameManager';
import { DeathScreen } from '../ui/DeathScreen';
import { LeaderboardController } from '../ui/LeaderboardController';
import { StatusHud } from '../ui/StatusHud';
import { UsernameChooser } from '../ui/UsernameChooser';

export class BlackholioScene extends Phaser.Scene {
  private gameManager?: GameManager;
  private cameraController?: CameraController;
  private leaderboard?: LeaderboardController;
  private statusHud?: StatusHud;

  constructor() {
    super('blackholio');
  }

  create(): void {
    this.cameras.main.setBackgroundColor('#050817');

    let manager: GameManager;
    const usernameChooser = new UsernameChooser(name => manager.enterGame(name));
    const deathScreen = new DeathScreen(() => manager.respawn());
    const statusHud = new StatusHud();
    const leaderboard = new LeaderboardController();

    manager = new GameManager(
      this,
      deathScreen,
      usernameChooser,
      statusHud,
      worldSize => this.setupArena(worldSize)
    );
    this.gameManager = manager;
    this.cameraController = new CameraController(this, manager);
    this.leaderboard = leaderboard;
    this.statusHud = statusHud;
    manager.connect();
  }

  update(time: number, delta: number): void {
    const manager = this.gameManager;
    if (!manager) {
      return;
    }
    manager.update(time, delta);
    this.cameraController?.update(delta);
    this.leaderboard?.update(manager.players.values(), manager.localPlayer);
    this.statusHud?.update(manager.localPlayer);
  }

  private setupArena(worldSize: number): void {
    const graphics = this.add.graphics().setDepth(-10);
    graphics.fillStyle(0x070e24, 1);
    graphics.fillRect(0, 0, worldSize, worldSize);
    graphics.lineStyle(5, 0xdda63e, 1);
    graphics.strokeRect(0, 0, worldSize, worldSize);
    this.cameras.main.centerOn(worldSize / 2, worldSize / 2);
    this.cameras.main.setZoom(
      Math.min(this.scale.width, this.scale.height) / worldSize
    );
    this.cameraController?.setWorldSize(worldSize);
  }
}
