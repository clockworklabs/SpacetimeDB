using System;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class CircleController : MonoBehaviour
{
    const float LERP_DURATION_SEC = 0.1f;

    public Renderer rend;
    public TMPro.TextMeshProUGUI usernameDisplay;

    private float lerpTimePassed;
    private Vector3 lerpStartPosition;

    private Vector3 lerpTargetPosition;
    private Vector3 lerpTargetScale;
    private Identity playerIdentity;

    private uint entityId;

    private static readonly int MainTexProperty = Shader.PropertyToID("_MainTex");

    public void Spawn(Circle circle)
    {
        var player = GameManager.conn.Db.Player.PlayerId.Find(circle.PlayerId);
        entityId = circle.EntityId;
        playerIdentity = player.Identity;

        var entity = GameManager.conn.Db.Entity.Id.Find(circle.EntityId);
		lerpStartPosition = lerpTargetPosition = transform.position = new Vector2
        {
            x = entity.Position.X,
            y = entity.Position.Y,
        };

        var playerRadius = GameManager.MassToRadius(entity.Mass);
		lerpTargetScale = transform.localScale = new Vector3
        {
            x = playerRadius * 2,
            y = playerRadius * 2,
            z = playerRadius * 2,
        };
        rend.material.SetColor(MainTexProperty, GameManager.GetRandomPlayerColor(circle.PlayerId));
        usernameDisplay.text = player.Name;
    }

    public uint GetEntityId() => entityId;
    public Entity GetEntity() => GameManager.conn.Db.Entity.Id.Find(entityId);

    public void Despawn()
    {
        Destroy(gameObject);
    }

    public void UpdatePosition(Entity entity)
    {
		lerpTimePassed = 0.0f;
		lerpStartPosition = transform.position;
		lerpTargetPosition = new Vector2
		{
			x = entity.Position.X,
			y = entity.Position.Y,
		};

		var playerRadius = GameManager.MassToRadius(entity.Mass);
        lerpTargetScale = new Vector3
        {
            x = playerRadius * 2,
            y = playerRadius * 2,
            z = playerRadius * 2,
        };
    }

    public void Update()
    {
        // Interpolate positions
        lerpTimePassed = Mathf.Min(lerpTimePassed + Time.deltaTime, LERP_DURATION_SEC);
        transform.position = Vector3.Lerp(lerpStartPosition, lerpTargetPosition, lerpTimePassed / LERP_DURATION_SEC);

        transform.localScale = Vector3.MoveTowards(transform.localScale, lerpTargetScale, 0.2f);
    }
}