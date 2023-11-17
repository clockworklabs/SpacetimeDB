using System;
using System.Collections.Generic;
using System.Security.Cryptography.X509Certificates;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using Vector2 = SpacetimeDB.Types.Vector2;

public class PlayerController : MonoBehaviour
{
    public int updatesPerSecond = 20;
    public int lerpUpdatesPerSecond = 5;
    public float targetCameraSize = 50;
    public Renderer rend;
    public TMPro.TextMeshProUGUI usernameDisplay;

    private float? lastMovementSendUpdate;
    private float lerpTimePassed;
    private Vector3 positionLerp1;
    private Vector3 positionLerp2;

    private Vector3? targetLerpPosition;

    public Identity? identity;

    private Vector3? targetPosition;
    private float? targetPositionReceiveUpdateTime;
    private Vector3? targetScale;
    private float? previousCameraSize;
    private float? lastRowUpdate;

    public static Identity localIdentity;
    public static PlayerController Local;

    public static Dictionary<uint, PlayerController> playersByEntityId = new Dictionary<uint, PlayerController>();

    public void Spawn(Circle circle)
    {
        var player = Player.FilterByEntityId(circle.EntityId);
        identity = player.PlayerId;
        playersByEntityId[player.EntityId] = this;

        if (localIdentity == identity)
        {
            Local = this;
        }

        var entity = Entity.FilterById(circle.EntityId);
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
        rend.material.color = GameManager.GetRandomColor(entity.Id);
        usernameDisplay.text = player.Name;
    }
    
    public void Despawn()
    {
        Destroy(gameObject);
        if (identity.HasValue)
        {
            var player = Player.FilterByPlayerId(identity.Value);
            playersByEntityId.Remove(player.EntityId);
        }
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

        previousCameraSize = targetCameraSize = playerRadius * 2 + 50.0f;
    }

    private void OnGUI()
    {
        if (identity.HasValue && localIdentity == identity.Value)
        {
            var player = Player.FilterByPlayerId(identity.Value);
            if (player == null)
            {
                return;
            }
            var entity = Entity.FilterById(player.EntityId);
            if (entity == null)
            {
                return;
            }

            GUI.Label(new Rect(0, 0, 100, 50), $"Mass: {entity.Mass}");
        }
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

        if (localIdentity == identity && previousCameraSize.HasValue)
        {
            GameManager.localCamera.orthographicSize =
                Mathf.Lerp(previousCameraSize.Value, targetCameraSize, Time.time / 10);
        }

        if (!identity.HasValue || localIdentity != identity.Value ||
            (lastMovementSendUpdate.HasValue && Time.time - lastMovementSendUpdate.Value < 1.0f / updatesPerSecond))
        {
            return;
        }

        lastMovementSendUpdate = Time.time;

        var mousePosition = new UnityEngine.Vector2
        {
            x = Input.mousePosition.x,
            y = Input.mousePosition.y,
        };
        var screenSize = new UnityEngine.Vector2
        {
            x = Screen.width,
            y = Screen.height,
        };
        var centerOfScreen = new UnityEngine.Vector2
        {
            x = Screen.width / 2.0f,
            y = Screen.height / 2.0f,
        };
        var direction = (mousePosition - centerOfScreen) / (screenSize.y / 3);
        var magnitude = Mathf.Clamp01(direction.magnitude);
        Reducer.UpdatePlayerInput(new Vector2
        {
            X = direction.x,
            Y = direction.y,
        }, magnitude);
    }

    public bool IsLocalPlayer() => identity.HasValue && identity.Value == localIdentity;
}