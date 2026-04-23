using Godot;
using SpacetimeDB.Types;

public partial class Instantiator : Node
{
    [Export]
    public PackedScene CircleScene { get; set; }

    [Export]
    public PackedScene FoodScene { get; set; }

    [Export]
    public PackedScene PlayerScene { get; set; }

    public CircleController SpawnCircle(Circle circle, PlayerController owner)
    {
        var entityController = InstantiateNode<CircleController>(CircleScene, $"Circle - {circle.EntityId}");
        entityController.Spawn(circle, owner);
        owner.OnCircleSpawned(entityController);
        return entityController;
    }

    public FoodController SpawnFood(Food food)
    {
        var entityController = InstantiateNode<FoodController>(FoodScene, $"Food - {food.EntityId}");
        entityController.Spawn(food);
        return entityController;
    }

    public PlayerController SpawnPlayer(Player player)
    {
        var playerController = InstantiateNode<PlayerController>(PlayerScene, $"PlayerController - {player.Name}");
        playerController.Initialize(player);
        return playerController;
    }

    private T InstantiateNode<T>(PackedScene scene, string nodeName) where T : Node, new()
    {
        var node = scene?.Instantiate<T>() ?? new T();
        node.Name = nodeName;
        AddChild(node);
        return node;
    }
}
