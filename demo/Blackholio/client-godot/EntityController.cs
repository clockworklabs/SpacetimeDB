using Godot;
using SpacetimeDB.Types;

public partial class EntityController : Circle2D
{
	private const float LerpDurationSec = 0.1f;

	public int EntityId { get; protected set; }

	protected float LerpTime { get; set; }
	protected Vector2 LerpStartPosition { get; set; }
	protected Vector2 TargetPosition { get; set; }
	protected float TargetRadius { get; set; } = 1;

	protected void SpawnEntity(int entityId)
	{
		EntityId = entityId;

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

	public virtual void OnDelete(EventContext context)
	{
		QueueFree();
	}

	public override void _Process(double delta)
	{
		var frameDelta = (float)delta;
		LerpTime = Mathf.Min(LerpTime + frameDelta, LerpDurationSec);
		GlobalPosition = LerpStartPosition.Lerp(TargetPosition, LerpTime / LerpDurationSec);
		Radius = Mathf.Lerp(Radius, TargetRadius, frameDelta * 8.0f);
	}

	public static float MassToRadius(int mass) => Mathf.Sqrt(mass);
}
