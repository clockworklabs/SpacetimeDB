import Phaser from 'phaser';
import type { Entity, Food } from '../module_bindings/types';
import { EntityController } from './EntityController';

const COLOR_PALETTE = [
  0x77fcad, 0x4cfa92, 0x23f678, 0x77fbc9, 0x4cf9b8, 0x23f5a5,
];

export class FoodController extends EntityController {
  constructor(scene: Phaser.Scene, entity: Entity, food: Food) {
    super(
      scene,
      entity,
      COLOR_PALETTE[Math.abs(food.entityId) % COLOR_PALETTE.length]
    );
  }
}
