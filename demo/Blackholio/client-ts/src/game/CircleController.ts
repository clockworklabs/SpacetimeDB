import Phaser from 'phaser';
import type { Circle, Entity } from '../module_bindings/types';
import { EntityController } from './EntityController';
import type { PlayerController } from './PlayerController';

const COLOR_PALETTE = [
  0xaf9f31, 0xaf7431, 0x702ffc, 0x335bfc, 0xb03636, 0xb06d36, 0x8d2b63,
  0x02bcfa, 0x0732fb, 0x021c92,
];
const LABEL_TEXTURE_FONT_SIZE = 24;
const LABEL_WORLD_SIZE_FACTOR = 0.4;

export class CircleController extends EntityController {
  private readonly label: Phaser.GameObjects.Text;

  constructor(
    scene: Phaser.Scene,
    entity: Entity,
    circle: Circle,
    private readonly owner: PlayerController
  ) {
    super(
      scene,
      entity,
      COLOR_PALETTE[Math.abs(circle.playerId) % COLOR_PALETTE.length]
    );
    this.label = scene.add
      .text(0, 0, owner.username, {
        color: '#ffffff',
        fontSize: `${LABEL_TEXTURE_FONT_SIZE}px`,
        fontStyle: 'bold',
      })
      .setOrigin(0.5, 0.5)
      .setResolution(2);
    this.container.add(this.label);
    owner.onCircleSpawned(this);
  }

  updateUsername(): void {
    this.label.setText(this.owner.username);
  }

  protected override draw(): void {
    super.draw();
    this.shape.lineStyle(0.6, 0xffffff, 0.5);
    this.shape.strokeCircle(0, 0, this.radius);
    const worldFontSize = Math.max(1, this.radius * LABEL_WORLD_SIZE_FACTOR);
    this.label.setScale(worldFontSize / LABEL_TEXTURE_FONT_SIZE);
  }
}
