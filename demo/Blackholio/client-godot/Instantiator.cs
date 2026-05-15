using System.Collections.Generic;
using Godot;
using SpacetimeDB.Types;

public partial class Instantiator : Node
{
    private DbConnection _conn;
    private DbConnection Conn
    {
        get => _conn;
        set
        {
            if (value == _conn) return;

            if (_conn != null)
            {
                _conn.Db.Circle.OnInsert -= CircleOnInsert;
                _conn.Db.ConsumeEntityEvent.OnInsert -= ConsumeEntityEventOnInsert;
                _conn.Db.Entity.OnUpdate -= EntityOnUpdate;
                _conn.Db.Entity.OnDelete -= EntityOnDelete;
                _conn.Db.Food.OnInsert -= FoodOnInsert;
                _conn.Db.Player.OnInsert -= PlayerOnInsert;
                _conn.Db.Player.OnDelete -= PlayerOnDelete;
            }
            
            _conn = value;

            if (value != null)
            {
                value.Db.Circle.OnInsert += CircleOnInsert;
                value.Db.ConsumeEntityEvent.OnInsert += ConsumeEntityEventOnInsert;
                value.Db.Entity.OnUpdate += EntityOnUpdate;
                value.Db.Entity.OnDelete += EntityOnDelete;
                value.Db.Food.OnInsert += FoodOnInsert;
                value.Db.Player.OnInsert += PlayerOnInsert;
                value.Db.Player.OnDelete += PlayerOnDelete;
            }
        }
    }
    
    private static Dictionary<int, EntityController> Entities { get; } = new();
    private static Dictionary<int, PlayerController> Players { get; } = new();
    private static HashSet<int> PendingConsumeAnimations { get; } = new();
    public static IReadOnlyDictionary<int, PlayerController> PlayerControllers => Players;
    
    public Instantiator(DbConnection conn)
    {
        Entities.Clear();
        Players.Clear();
        PendingConsumeAnimations.Clear();
        Conn = conn;
    }

    public override void _ExitTree()
    {
        GD.Print("Instantiator Exit Tree");
        Conn = null;
    }

    private void CircleOnInsert(EventContext context, Circle insertedValue)
    {
        var player = GetOrCreatePlayer(insertedValue.PlayerId);
        var entityController = SpawnCircle(insertedValue, player);
        Entities[insertedValue.EntityId] = entityController;
    }

    private void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
    {
        if (Entities.TryGetValue(newEntity.EntityId, out var entityController))
        {
            entityController.OnEntityUpdated(newEntity);
        }
    }

    private void EntityOnDelete(EventContext context, Entity oldEntity)
    {
        if (Entities.Remove(oldEntity.EntityId, out var entityController))
        {
            if (PendingConsumeAnimations.Remove(oldEntity.EntityId))
            {
                entityController.OnConsumed();
                return;
            }

            entityController.OnDelete();
        }
    }

    private void ConsumeEntityEventOnInsert(EventContext context, ConsumeEntityEvent evt)
    {
        if (!Entities.TryGetValue(evt.ConsumedEntityId, out var consumedEntity) ||
            !Entities.TryGetValue(evt.ConsumerEntityId, out var consumerEntity))
        {
            return;
        }

        PendingConsumeAnimations.Add(evt.ConsumedEntityId);
        consumedEntity.StartDespawn(consumerEntity);
    }

    private void FoodOnInsert(EventContext context, Food insertedValue)
    {
        var entityController = SpawnFood(insertedValue);
        Entities[insertedValue.EntityId] = entityController;
    }

    private void PlayerOnInsert(EventContext context, Player insertedPlayer)
    {
        GetOrCreatePlayer(insertedPlayer.PlayerId);
    }

    private void PlayerOnDelete(EventContext context, Player deletedValue)
    {
        if (Players.Remove(deletedValue.PlayerId, out var playerController))
        {
            playerController.QueueFree();
        }
    }

    private PlayerController GetOrCreatePlayer(int playerId)
    {
        if (!Players.TryGetValue(playerId, out var playerController))
        {
            var player = Conn.Db.Player.PlayerId.Find(playerId);
            playerController = SpawnPlayer(player);
            Players[playerId] = playerController;
        }

        return playerController;
    }

    private CircleController SpawnCircle(Circle circle, PlayerController owner)
    {
        var entityController = new CircleController(circle, owner)
        {
            Name = $"Circle - {circle.EntityId}",
        };
        
        AddChild(entityController);
        
        return entityController;
    }

    private FoodController SpawnFood(Food food)
    {
        var entityController = new FoodController(food)
        {
            Name = $"Food - {food.EntityId}",
        };
        
        AddChild(entityController);
        
        return entityController;
    }

    private PlayerController SpawnPlayer(Player player)
    {
        var playerController = new PlayerController(player)
        {
            Name = $"Player - {player.Name}"
        };
        
        AddChild(playerController);
        
        return playerController;
    }
}
