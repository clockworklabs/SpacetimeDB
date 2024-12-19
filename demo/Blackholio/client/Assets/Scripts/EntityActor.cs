using SpacetimeDB.Types;
using System;
using System.Collections.Generic;
using System.Linq;
using Unity.VisualScripting;
using UnityEngine;

public abstract class EntityActor : MonoBehaviour
{
	const float LERP_DURATION_SEC = 0.1f;

	private static readonly int ShaderColorProperty = Shader.PropertyToID("_Color");

	[DoNotSerialize] public uint EntityId;

	protected float LerpTime;
	protected Vector3 LerpStartPosition;
	protected Vector3 LerpTargetPositio;
	protected Vector3 TargetScale;



	protected virtual void Spawn(uint entityId)
	{
		EntityId = entityId;

		var entity = ConnectionManager.Conn.Db.Entity.EntityId.Find(entityId);
		LerpStartPosition = LerpTargetPositio = transform.position = (Vector2)entity.Position;
		transform.localScale = Vector3.one;
		TargetScale = MassToScale(entity.Mass);
	}

	public void SetColor(Color color)
	{
		GetComponent<SpriteRenderer>().material.SetColor(ShaderColorProperty, color);
	}

	public virtual void OnUpdate(Entity newVal)
	{
		LerpTime = 0.0f;
		LerpStartPosition = transform.position;
		LerpTargetPositio = (Vector2)newVal.Position;
		TargetScale = MassToScale(newVal.Mass);
	}

	public virtual void OnDelete()
	{
		Destroy(gameObject);
	}

	public virtual void Update()
	{
		//Interpolate position and scale
		LerpTime = Mathf.Min(LerpTime + Time.deltaTime, LERP_DURATION_SEC);
		transform.position = Vector3.Lerp(LerpStartPosition, LerpTargetPositio, LerpTime / LERP_DURATION_SEC);
		transform.localScale = Vector3.Lerp(transform.localScale, TargetScale, Time.deltaTime * 8);
	}



	public static Vector3 MassToScale(uint mass)
	{
		var diameter = MassToDiameter(mass);
		return new Vector3(diameter, diameter, 1);
	}

	public static float MassToRadius(uint mass) => Mathf.Sqrt(mass);
	public static float MassToDiameter(uint mass) => MassToRadius(mass) * 2;
}