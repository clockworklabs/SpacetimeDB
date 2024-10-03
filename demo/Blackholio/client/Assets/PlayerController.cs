using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using Vector2 = SpacetimeDB.Types.Vector2;

public class PlayerController : MonoBehaviour
{
    private Dictionary<uint, CircleController> circlesByEntityId = new Dictionary<uint, CircleController>();

    public float targetCameraSize = 50;
    public int updatesPerSecond = 20;

    private Identity identity;
    private uint playerId;
    private float? previousCameraSize;
    private float? lastMovementSendUpdate;
    public static PlayerController Local;

    public void Spawn(Identity identity)
    {
        this.identity = identity;
        playerId = GameManager.conn.RemoteTables.player.FindByIdentity(identity)!.PlayerId;
        if (IsLocalPlayer())
        {
            Local = this;
        }
    }

    private void OnDestroy()
    {
        // If we have any circles, destroy them
        var circles = circlesByEntityId.Values.ToList();
        foreach (var circle in circles)
        {
            Destroy(circle.gameObject);
        }
        circlesByEntityId.Clear();
    }

    public void SpawnCircle(Circle insertedCircle, CircleController circlePrefab)
    {
        var circle = Instantiate(circlePrefab);
        circle.Spawn(insertedCircle);
        circlesByEntityId[insertedCircle.EntityId] = circle;
    }

    public void DespawnCircle(Circle deletedCircle)
    {
        // This means we got eaten
        if (circlesByEntityId.TryGetValue(deletedCircle.EntityId, out var circle))
        {
            circlesByEntityId.Remove(deletedCircle.EntityId);
            // If the local player died, show the death screen
            circle.Despawn();
        }

        // If the player has no more circles remaining, show the death screen
        if (IsLocalPlayer() && GameManager.conn.RemoteTables.circle.FindByEntityId(playerId) == null)
        {
            GameManager.instance.deathScreen.SetActive(true);
        }
    }

    public void CircleUpdate(Entity oldCircle, Entity newCircle)
    {
        if (!circlesByEntityId.TryGetValue(newCircle.Id, out var circle))
        {
            return;
        }

        circle.UpdatePosition(newCircle);
        var playerRadius = GameManager.MassToRadius(TotalMass());
        previousCameraSize = targetCameraSize = playerRadius * 2 + 50.0f;
    }

    public uint TotalMass()
    {
        uint mass = 0;
        foreach (var circle in circlesByEntityId.Values)
        {
            var entity = GameManager.conn.RemoteTables.entity.FindById(circle.GetEntityId());
            // If this entity is being deleted on the same frame that we're moving, we can have a null entity here.
            if (entity == null)
            {
                continue;
            }

            mass += entity.Mass;
        }

        return mass;
    }

    public string GetUsername() => GameManager.conn.RemoteTables.player.FindByIdentity(identity)!.Name;

    private void OnGUI()
    {
        if (!IsLocalPlayer())
        {
            return;
        }

        GUI.Label(new Rect(0, 0, 100, 50), $"Total Mass: {TotalMass()}");
    }

    public bool IsLocalPlayer() => GameManager.localIdentity != null && identity == GameManager.localIdentity;

    public UnityEngine.Vector2? CenterOfMass()
    {
        if (circlesByEntityId.Count == 0)
        {
            return null;
        }
        
        var circles = circlesByEntityId.Values;        
        float totalX = 0, totalY = 0;
        float totalMass = 0;
        foreach (var circle in circles)
        {
            var entity = circle.GetEntity();
            var position = circle.transform.position;
            totalX += position.x * entity.Mass;
            totalY += position.y * entity.Mass;
            totalMass += entity.Mass;
        }

        return new UnityEngine.Vector2(totalX / totalMass, totalY / totalMass);
    }
    
    public void Update()
    {
        if (IsLocalPlayer() && Input.GetKeyDown(KeyCode.Space))
        {
            GameManager.conn.RemoteReducers.PlayerSplit();
            Debug.LogWarning("Player Split!");
        }
        
        if (IsLocalPlayer() && previousCameraSize.HasValue)
        {
            GameManager.localCamera.orthographicSize =
                Mathf.Lerp(previousCameraSize.Value, targetCameraSize, Time.time / 10);
        }

        if (!IsLocalPlayer() ||
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
        GameManager.conn.RemoteReducers.UpdatePlayerInput(new Vector2
        {
            X = direction.x,
            Y = direction.y,
        }, magnitude);
    }
}