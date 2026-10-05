import Phaser from 'phaser';
import type { Entity } from '../module_bindings/types';
import { massToRadius, type Vec2 } from './math';

const LERP_DURATION_MS = 100;

export abstract class EntityController {
  public readonly entityId: number;
  public consuming = false;

  protected readonly shape: Phaser.GameObjects.Graphics;
  protected readonly container: Phaser.GameObjects.Container;
  protected radius = 0;

  private readonly target: Phaser.Math.Vector2;
  private targetRadius: number;

  protected constructor(
    protected readonly scene: Phaser.Scene,
    entity: Entity,
    protected readonly color: number
  ) {
    this.entityId = entity.entityId;
    this.shape = scene.add.graphics();
    this.container = scene.add.container(entity.position.x, entity.position.y, [
      this.shape,
    ]);
    this.target = new Phaser.Math.Vector2(entity.position.x, entity.position.y);
    this.targetRadius = massToRadius(entity.mass);
  }

  get position(): Vec2 {
    return { x: this.container.x, y: this.container.y };
  }

  onEntityUpdated(entity: Entity): void {
    if (this.consuming) {
      return;
    }
    this.target.set(entity.position.x, entity.position.y);
    this.targetRadius = massToRadius(entity.mass);
  }

  onDelete(): void {
    this.container.destroy(true);
  }

  despawnToward(target: EntityController): void {
    this.consuming = true;
    this.scene.tweens.add({
      targets: this.container,
      duration: 200,
      x: target.container.x,
      y: target.container.y,
      scale: 0,
      onComplete: () => this.container.destroy(true),
    });
  }

  update(delta: number): void {
    if (this.consuming) {
      return;
    }
    const positionT = Math.min(1, delta / LERP_DURATION_MS);
    const radiusT = Math.min(1, (delta / 1000) * 8);
    this.container.x = Phaser.Math.Linear(this.container.x, this.target.x, positionT);
    this.container.y = Phaser.Math.Linear(this.container.y, this.target.y, positionT);
    this.radius = Phaser.Math.Linear(this.radius, this.targetRadius, radiusT);
    this.draw();
  }

  protected draw(): void {
    this.shape.clear();
    this.shape.fillStyle(this.color, 1);
    this.shape.fillCircle(0, 0, this.radius);
  }
}
