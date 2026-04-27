using Godot;
using SpacetimeDB.Types;

public abstract partial class EntityController : Circle2D
{
    private const float LerpDurationSec = 0.1f;

    public int EntityId { get; private set; }

    private float LerpTime { get; set; }
    private Vector2 LerpStartPosition { get; set; }
    private Vector2 TargetPosition { get; set; }
    private float TargetRadius { get; set; } = 1;

    protected EntityController(int entityId, Color color)
    {
        EntityId = entityId;
        Color = color;

        var entity = GameManager.Conn.Db.Entity.EntityId.Find(entityId);
        var position = (Vector2)entity.Position;
        LerpStartPosition = position;
        TargetPosition = position;
        GlobalPosition = position;
        Radius = 0;
        TargetRadius = MassToRadius(entity.Mass);
    }

    public void OnEntityUpdated(Entity newVal)
    {
        LerpTime = 0.0f;
        LerpStartPosition = GlobalPosition;
        TargetPosition = (Vector2)newVal.Position;
        TargetRadius = MassToRadius(newVal.Mass);
    }

    public virtual void OnDelete() => QueueFree();

    public override void _Process(double delta)
    {
        var frameDelta = (float)delta;
        LerpTime = Mathf.Min(LerpTime + frameDelta, LerpDurationSec);
        GlobalPosition = LerpStartPosition.Lerp(TargetPosition, LerpTime / LerpDurationSec);
        Radius = Mathf.Lerp(Radius, TargetRadius, frameDelta * 8.0f);
    }

    private static float MassToRadius(int mass) => Mathf.Sqrt(mass);
}
