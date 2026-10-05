import type Phaser from 'phaser';
import type { Identity } from 'spacetimedb';
import { DbConnection, type ErrorContext } from '../module_bindings';
import type {
  Circle,
  ConsumeEntityEvent,
  Entity,
  Food,
  Player,
} from '../module_bindings/types';
import type { DeathScreen } from '../ui/DeathScreen';
import type { StatusHud } from '../ui/StatusHud';
import type { UsernameChooser } from '../ui/UsernameChooser';
import { CircleController } from './CircleController';
import { EntityController } from './EntityController';
import { FoodController } from './FoodController';
import { PlayerController } from './PlayerController';

const HOST = import.meta.env.VITE_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.VITE_SPACETIMEDB_DB_NAME ?? 'blackholio';
const TOKEN_KEY = `${HOST}/${DB_NAME}/auth_token`;

export class GameManager {
  public connection?: DbConnection;
  public identity?: Identity;
  public readonly entities = new Map<number, EntityController>();
  public readonly players = new Map<number, PlayerController>();

  private readonly pendingConsumeAnimations = new Set<number>();

  constructor(
    public readonly scene: Phaser.Scene,
    public readonly deathScreen: DeathScreen,
    private readonly usernameChooser: UsernameChooser,
    private readonly statusHud: StatusHud,
    private readonly setupArena: (worldSize: number) => void
  ) {}

  get localPlayer(): PlayerController | undefined {
    return Array.from(this.players.values()).find(player => player.isLocalPlayer);
  }

  connect(): void {
    const storedToken = localStorage.getItem(TOKEN_KEY) || undefined;
    const builder = DbConnection.builder()
      .withUri(HOST)
      .withDatabaseName(DB_NAME)
      .onConnect((connection: DbConnection, identity: Identity, token: string) => {
        this.connection = connection;
        this.identity = identity;
        localStorage.setItem(TOKEN_KEY, token);
        this.statusHud.setStatus('Connected', 'connected');
        this.registerCallbacks(connection);
        connection
          .subscriptionBuilder()
          .onApplied(() => this.handleSubscriptionApplied())
          .subscribeToAllTables();
      })
      .onDisconnect(() => {
        this.statusHud.setStatus('Disconnected', 'error');
      })
      .onConnectError((_ctx: ErrorContext, error: Error) => {
        this.statusHud.setStatus(`Error: ${error.message}`, 'error');
      });
    if (storedToken) {
      builder.withToken(storedToken);
    }
    builder.build();
  }

  enterGame(name: string): void {
    void this.connection?.reducers.enterGame({ name });
    this.usernameChooser.show(false);
    this.deathScreen.setVisible(false);
  }

  respawn(): void {
    void this.connection?.reducers.respawn({});
    this.deathScreen.setVisible(false);
  }

  findEntity(entityId: number): Entity | undefined {
    return this.connection?.db.entity.entityId.find(entityId) ?? undefined;
  }

  update(time: number, delta: number): void {
    this.entities.forEach(entity => entity.update(delta));
    this.localPlayer?.update(time);
  }

  private registerCallbacks(connection: DbConnection): void {
    connection.db.entity.onInsert((_ctx, entity) => this.syncEntity(entity.entityId));
    connection.db.entity.onUpdate((_ctx, _oldEntity, entity) => {
      this.entities.get(entity.entityId)?.onEntityUpdated(entity);
    });
    connection.db.entity.onDelete((_ctx, entity) => this.entityOnDelete(entity));
    connection.db.circle.onInsert((_ctx, circle) => this.circleOnInsert(circle));
    connection.db.circle.onDelete((_ctx, circle) => this.circleOnDelete(circle));
    connection.db.food.onInsert((_ctx, food) => this.foodOnInsert(food));
    connection.db.player.onInsert((_ctx, player) => this.playerOnInsert(player));
    connection.db.player.onUpdate((_ctx, _oldPlayer, player) =>
      this.playerOnUpdate(player)
    );
    connection.db.player.onDelete((_ctx, player) => {
      this.players.delete(player.playerId);
    });
    connection.db.consumeEntityEvent.onInsert((_ctx, event) =>
      this.consumeEntityEventOnInsert(event)
    );
  }

  private handleSubscriptionApplied(): void {
    const connection = this.connection;
    if (!connection || !this.identity) {
      return;
    }
    const config = connection.db.config.id.find(0);
    if (config) {
      this.setupArena(Number(config.worldSize));
    }
    this.syncSubscribedState();
    const player = connection.db.player.identity.find(this.identity);
    if (!player || !player.name) {
      this.usernameChooser.show(true);
      return;
    }
    const hasCircle = Array.from(connection.db.circle.iter()).some(
      circle => circle.playerId === player.playerId
    );
    if (hasCircle) {
      this.usernameChooser.show(false);
    } else {
      void connection.reducers.enterGame({ name: player.name });
    }
  }

  private syncSubscribedState(): void {
    const connection = this.connection;
    if (!connection) {
      return;
    }
    for (const player of connection.db.player.iter()) {
      this.playerOnUpdate(player);
    }
    for (const circle of connection.db.circle.iter()) {
      this.circleOnInsert(circle);
    }
    for (const food of connection.db.food.iter()) {
      this.foodOnInsert(food);
    }
  }

  private circleOnInsert(circle: Circle): void {
    if (this.entities.has(circle.entityId)) {
      return;
    }
    const entity = this.findEntity(circle.entityId);
    const player = this.getOrCreatePlayer(circle.playerId);
    if (!entity || !player) {
      return;
    }
    this.entities.set(
      circle.entityId,
      new CircleController(this.scene, entity, circle, player)
    );
  }

  private foodOnInsert(food: Food): void {
    if (this.entities.has(food.entityId)) {
      return;
    }
    const entity = this.findEntity(food.entityId);
    if (entity) {
      this.entities.set(
        food.entityId,
        new FoodController(this.scene, entity, food)
      );
    }
  }

  private circleOnDelete(circle: Circle): void {
    this.players.get(circle.playerId)?.onCircleDeleted(circle.entityId);
  }

  private syncEntity(entityId: number): void {
    const connection = this.connection;
    if (!connection || this.entities.has(entityId)) {
      return;
    }
    const circle = connection.db.circle.entityId.find(entityId);
    if (circle) {
      this.circleOnInsert(circle);
      return;
    }
    const food = connection.db.food.entityId.find(entityId);
    if (food) {
      this.foodOnInsert(food);
    }
  }

  private entityOnDelete(entity: Entity): void {
    const entityController = this.entities.get(entity.entityId);
    if (!entityController) {
      return;
    }
    this.entities.delete(entity.entityId);
    if (this.pendingConsumeAnimations.delete(entity.entityId)) {
      return;
    }
    entityController.onDelete();
  }

  private consumeEntityEventOnInsert(event: ConsumeEntityEvent): void {
    const consumedEntity = this.entities.get(event.consumedEntityId);
    const consumerEntity = this.entities.get(event.consumerEntityId);
    if (!consumedEntity || !consumerEntity) {
      return;
    }
    this.pendingConsumeAnimations.add(event.consumedEntityId);
    consumedEntity.despawnToward(consumerEntity);
  }

  private playerOnInsert(player: Player): void {
    this.getOrCreatePlayer(player.playerId);
    for (const circle of this.connection?.db.circle.iter() ?? []) {
      if (circle.playerId === player.playerId) {
        this.circleOnInsert(circle);
      }
    }
  }

  private playerOnUpdate(player: Player): void {
    const controller = this.getOrCreatePlayer(player.playerId);
    controller?.updatePlayer(player);
  }

  private getOrCreatePlayer(playerId: number): PlayerController | undefined {
    const existing = this.players.get(playerId);
    if (existing) {
      return existing;
    }
    const player = Array.from(this.connection?.db.player.iter() ?? []).find(
      value => value.playerId === playerId
    );
    if (!player) {
      return undefined;
    }
    const controller = new PlayerController(this, player);
    this.players.set(playerId, controller);
    return controller;
  }
}
