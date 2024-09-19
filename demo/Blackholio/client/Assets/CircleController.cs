using System;
using System.Collections.Generic;
using System.Security.Cryptography.X509Certificates;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using Vector2 = SpacetimeDB.Types.Vector2;

public class CircleController : MonoBehaviour
{
    public int lerpUpdatesPerSecond = 5;
    public Renderer rend;
    public TMPro.TextMeshProUGUI usernameDisplay;

    private float lerpTimePassed;
    private Vector3 positionLerp1;
    private Vector3 positionLerp2;

    private Vector3? targetLerpPosition;

    private Vector3? targetPosition;
    private float? targetPositionReceiveUpdateTime;
    private Vector3? targetScale;
    private float? lastRowUpdate;
    private Identity playerIdentity;

    private uint entityId;

    private static readonly int MainTexProperty = Shader.PropertyToID("_MainTex");

    public void Spawn(Circle circle)
    {
        var player = Player.FilterByPlayerId(circle.PlayerId);
        entityId = circle.EntityId;
        playerIdentity = player.Identity;

        var entity = Entity.FindById(circle.EntityId);
        targetPosition = positionLerp1 = positionLerp2 = transform.position = new UnityEngine.Vector2
        {
            x = entity.Position.X,
            y = entity.Position.Y,
        };

        var playerRadius = GameManager.MassToRadius(entity.Mass);
        targetScale = transform.localScale = new Vector3
        {
            x = playerRadius * 2,
            y = playerRadius * 2,
            z = playerRadius * 2,
        };
        rend.material.SetColor(MainTexProperty, GameManager.GetRandomColor(entity.Id));
        usernameDisplay.text = player.Name;
    }

    public uint GetEntityId() => entityId;
    public Entity GetEntity() => Entity.FilterById(entityId);

    public void Despawn()
    {
        Destroy(gameObject);
    }

    public void UpdatePosition(Entity entity)
    {
        targetPosition = new UnityEngine.Vector2
        {
            x = entity.Position.X,
            y = entity.Position.Y,
        };

        var playerRadius = GameManager.MassToRadius(entity.Mass);
        targetScale = new Vector3
        {
            x = playerRadius * 2,
            y = playerRadius * 2,
            z = playerRadius * 2,
        };
    }

    public void Update()
    {
        // Interpolate positions
        lerpTimePassed += Time.deltaTime;
        transform.position = Vector3.Lerp(positionLerp1, positionLerp2,
            lerpTimePassed / (1.0f / lerpUpdatesPerSecond));
        if (lerpTimePassed >= 1.0f / lerpUpdatesPerSecond && targetPosition.HasValue)
        {
            // Take new positions
            lerpTimePassed = 0.0f;
            positionLerp1 = transform.position;
            positionLerp2 = targetPosition.Value;
        }

        if (targetScale.HasValue)
        {
            transform.localScale = Vector3.MoveTowards(transform.localScale,
                targetScale.Value, 0.2f);
        }
    }
}