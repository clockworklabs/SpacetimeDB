using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class PlayerController : MonoBehaviour
{
    private Dictionary<uint, CircleController> circlesByEntityId = new Dictionary<uint, CircleController>();

    public int updatesPerSecond = 20;

    private Identity identity;
    private uint playerId;
    public float targetCameraSize;
    private float previousCameraSize;
    private float lastMovementSendUpdate;
    public static PlayerController Local;
    private bool testInputEnabled;
    private Vector2 testInput;
    private Vector2? lockInputPosition;

    public void SetTestInput(Vector2 input) => testInput = input;
    public void EnableTestInput() => testInputEnabled = true;
    
    public void Spawn(Identity identity)
    {
        this.identity = identity;
        playerId = GameManager.conn.Db.Player.Identity.Find(identity)!.PlayerId;
        if (IsLocalPlayer())
        {
            Local = this;
        }
        previousCameraSize = targetCameraSize = CalculateCameraSize();
    }

    private void OnDestroy()
    {
        // If we have any circles, destroy them
        var circles = circlesByEntityId.Values.ToList();
        foreach (var circle in circles)
        {
            if (circle != null)
            {
                Destroy(circle.gameObject);
            }
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
        if (IsLocalPlayer() && circlesByEntityId.Count == 0)
        {
            GameManager.instance.deathScreen.SetActive(true);
        }
    }

    public void CircleUpdate(Entity oldCircle, Entity newCircle)
    {
        if (!circlesByEntityId.TryGetValue(newCircle.EntityId, out var circle))
        {
            return;
        }

        circle.UpdatePosition(newCircle);
        previousCameraSize = GameManager.localCamera.orthographicSize;
        targetCameraSize = CalculateCameraSize();
    }

    private float CalculateCameraSize()
	{
        return 50f + Mathf.Min(50, TotalMass() / 5) + Mathf.Min(circlesByEntityId.Count - 1, 1) * 30;
	}

    public uint TotalMass()
    {
        uint mass = 0;
        foreach (var circle in circlesByEntityId.Values)
        {
            var entity = GameManager.conn.Db.Entity.EntityId.Find(circle.GetEntityId());
            // If this entity is being deleted on the same frame that we're moving, we can have a null entity here.
            if (entity == null)
            {
                continue;
            }

            mass += entity.Mass;
        }

        return mass;
    }

    public string GetUsername() => GameManager.conn.Db.Player.PlayerId.Find(playerId)!.Name;

    private void OnGUI()
    {
        if (!IsLocalPlayer() || !GameManager.IsConnected())
        {
            return;
        }

        GUI.Label(new Rect(0, 0, 100, 50), $"Total Mass: {TotalMass()}");
    }

    public bool IsLocalPlayer() => GameManager.localIdentity != null && identity == GameManager.localIdentity;

    public Vector2? CenterOfMass()
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

        return new Vector2(totalX / totalMass, totalY / totalMass);
    }
    
    public void Update()
    {
        if (!IsLocalPlayer())
        {
            return;
        }

        if (Input.GetKeyDown(KeyCode.Space))
        {
            GameManager.conn.Reducers.PlayerSplit();
        }

        if (Input.GetKeyDown(KeyCode.Q))
        {
            if (lockInputPosition.HasValue)
            {
                lockInputPosition = null;
			}
            else
            {
				lockInputPosition = (Vector2)Input.mousePosition;
            }
        }
        
        GameManager.localCamera.orthographicSize =
            Mathf.Lerp(GameManager.localCamera.orthographicSize, targetCameraSize, Time.deltaTime * 3);

        //Throttled input requests
        if (Time.time - lastMovementSendUpdate >= 1.0f / updatesPerSecond)
        {
            lastMovementSendUpdate = Time.time;

            var mousePosition = lockInputPosition ?? (Vector2)Input.mousePosition;
            var screenSize = new Vector2
            {
                x = Screen.width,
                y = Screen.height,
            };
            var centerOfScreen = screenSize / 2;

			var direction = (mousePosition - centerOfScreen) / (screenSize.y / 3);
            if (testInputEnabled) direction = testInput;
            GameManager.conn.Reducers.UpdatePlayerInput(new DbVector2
            {
                X = direction.x,
                Y = direction.y,
            });
        }
    }
}