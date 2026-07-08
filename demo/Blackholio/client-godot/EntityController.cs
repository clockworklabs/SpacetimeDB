using Godot;
using SpacetimeDB.Types;

public abstract partial class EntityController : Circle2D
{
	private const float LerpDurationSec = 0.1f;
	private const float DespawnDurationSec = 0.2f;

	public int EntityId { get; private set; }

	private float LerpTime { get; set; }
	private Vector2 LerpStartPosition { get; set; }
	private Vector2 TargetPosition { get; set; }
	private float TargetRadius { get; set; }
	private bool IsDespawning { get; set; }
	private float DespawnTime { get; set; }
	private Vector2 DespawnStartPosition { get; set; }
	private float DespawnStartRadius { get; set; }
	private Node2D DespawnTarget { get; set; }

	protected EntityController(int entityId, Color color)
	{
		EntityId = entityId;
		Color = color;
		AnimationSeed = entityId * 0.73f;

		var entity = GameManager.Conn.Db.Entity.EntityId.Find(entityId);
		var position = (Vector2)entity.Position;
		LerpStartPosition = position;
		TargetPosition = position;
		GlobalPosition = position;
		Radius = 0;
		TargetRadius = MassToRadius(entity.Mass);
	}

	public void OnEntityUpdated(Entity newRow)
	{
		if (IsDespawning) return;

		LerpTime = 0.0f;
		LerpStartPosition = GlobalPosition;
		TargetPosition = (Vector2)newRow.Position;
		TargetRadius = MassToRadius(newRow.Mass);
	}

	public virtual void OnDelete() => QueueFree();
	public virtual void OnConsumed() { }

	public void StartDespawn(Node2D target)
	{
		IsDespawning = true;
		DespawnTime = 0.0f;
		DespawnStartPosition = GlobalPosition;
		DespawnStartRadius = Radius;
		DespawnTarget = target;
		ZIndex += 10;
	}

	public override void _Process(double delta)
	{
		var frameDelta = (float)delta;
		if (IsDespawning)
		{
			DespawnTime = Mathf.Min(DespawnTime + frameDelta, DespawnDurationSec);
			var t = DespawnTime / DespawnDurationSec;
			var targetPosition = IsInstanceValid(DespawnTarget) ? DespawnTarget.GlobalPosition : TargetPosition;
			GlobalPosition = DespawnStartPosition.Lerp(targetPosition, t);
			Radius = Mathf.Lerp(DespawnStartRadius, 0.0f, t);
			RedrawAnimatedVisuals();

			if (DespawnTime >= DespawnDurationSec)
			{
				QueueFree();
			}

			return;
		}

		LerpTime = Mathf.Min(LerpTime + frameDelta, LerpDurationSec);
		GlobalPosition = LerpStartPosition.Lerp(TargetPosition, LerpTime / LerpDurationSec);
		Radius = Mathf.Lerp(Radius, TargetRadius, frameDelta * 8.0f);
		RedrawAnimatedVisuals();
	}

	private static float MassToRadius(int mass) => Mathf.Sqrt(mass);
}
