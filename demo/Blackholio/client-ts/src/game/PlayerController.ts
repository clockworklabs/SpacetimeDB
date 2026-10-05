import Phaser from 'phaser';
import type { Player } from '../module_bindings/types';
import { pointerDirection } from './input';
import { centerOfMass, type Vec2, type WeightedPosition } from './math';
import type { CircleController } from './CircleController';
import type { GameManager } from './GameManager';

const SEND_UPDATES_FREQUENCY_MS = 1000 / 20;

export class PlayerController {
  private readonly ownedCircles: CircleController[] = [];
  private readonly splitKey?: Phaser.Input.Keyboard.Key;
  private readonly lockKey?: Phaser.Input.Keyboard.Key;
  private readonly suicideKey?: Phaser.Input.Keyboard.Key;
  private lastMovementSendTimestamp = 0;
  private lockedPointer?: Vec2;

  constructor(
    private readonly gameManager: GameManager,
    private player: Player
  ) {
    const keyboard = gameManager.scene.input.keyboard;
    this.splitKey = keyboard?.addKey(Phaser.Input.Keyboard.KeyCodes.SPACE, false);
    this.lockKey = keyboard?.addKey(Phaser.Input.Keyboard.KeyCodes.Q, false);
    this.suicideKey = keyboard?.addKey(Phaser.Input.Keyboard.KeyCodes.S, false);
  }

  get playerId(): number {
    return this.player.playerId;
  }

  get username(): string {
    return this.player.name;
  }

  get isLocalPlayer(): boolean {
    return this.gameManager.identity?.isEqual(this.player.identity) ?? false;
  }

  get numberOfOwnedCircles(): number {
    return this.ownedCircles.length;
  }

  updatePlayer(player: Player): void {
    this.player = player;
    this.ownedCircles.forEach(circle => circle.updateUsername());
  }

  onCircleSpawned(circle: CircleController): void {
    this.ownedCircles.push(circle);
  }

  onCircleDeleted(entityId: number): void {
    const index = this.ownedCircles.findIndex(circle => circle.entityId === entityId);
    if (index !== -1) {
      this.ownedCircles.splice(index, 1);
      if (this.isLocalPlayer && this.ownedCircles.length === 0) {
        this.gameManager.deathScreen.setVisible(true);
      }
    }
  }

  totalMass(): number {
    return this.ownedCircles.reduce(
      (sum, circle) => sum + (this.gameManager.findEntity(circle.entityId)?.mass ?? 0),
      0
    );
  }

  centerOfMass(): Vec2 | undefined {
    const entities: WeightedPosition[] = this.ownedCircles.flatMap(circle => {
      const entity = this.gameManager.findEntity(circle.entityId);
      return entity ? [{ mass: entity.mass, position: circle.position }] : [];
    });
    return centerOfMass(entities);
  }

  update(time: number): void {
    if (!this.isLocalPlayer || this.ownedCircles.length === 0) {
      return;
    }
    if (this.splitKey && Phaser.Input.Keyboard.JustDown(this.splitKey)) {
      void this.gameManager.connection?.reducers.playerSplit({});
    }
    if (this.lockKey && Phaser.Input.Keyboard.JustDown(this.lockKey)) {
      this.lockedPointer = this.lockedPointer
        ? undefined
        : this.currentPointerPosition();
    }
    if (this.suicideKey && Phaser.Input.Keyboard.JustDown(this.suicideKey)) {
      void this.gameManager.connection?.reducers.suicide({});
    }
    if (time - this.lastMovementSendTimestamp < SEND_UPDATES_FREQUENCY_MS) {
      return;
    }
    this.lastMovementSendTimestamp = time;
    const direction = pointerDirection(
      this.lockedPointer ?? this.currentPointerPosition(),
      {
        x: this.gameManager.scene.scale.width,
        y: this.gameManager.scene.scale.height,
      }
    );
    void this.gameManager.connection?.reducers.updatePlayerInput({ direction });
  }

  private currentPointerPosition(): Vec2 {
    const pointer = this.gameManager.scene.input.activePointer;
    return { x: pointer.x, y: pointer.y };
  }
}
