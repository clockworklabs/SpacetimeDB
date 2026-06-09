import Phaser from 'phaser';
import './style.css';
import { BlackholioScene } from './game/BlackholioScene';

new Phaser.Game({
  type: Phaser.AUTO,
  parent: 'game',
  backgroundColor: '#050817',
  scale: {
    mode: Phaser.Scale.RESIZE,
    width: window.innerWidth,
    height: window.innerHeight,
  },
  scene: BlackholioScene,
});
