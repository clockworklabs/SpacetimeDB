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

    private float? previousCameraSize;
    private float? lastMovementSendUpdate;

    public void Spawn()
    {
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
    }

    public void CircleUpdate(Entity oldCircle, Entity newCircle)
    {
        if (!circlesByEntityId.TryGetValue(newCircle.Id, out var circle))
        {
            return;
        }

        circle.UpdatePosition(newCircle);
        var playerRadius = GameManager.MassToRadius(TotalMass());
        previousCameraSize = targetCameraSize = 100.0f;
    }

    public uint TotalMass()
    {
        uint mass = 0;
        foreach (var circle in circlesByEntityId.Values)
        {
            var entity = GameManager.conn.Db.Entity.Id.Find(circle.GetEntityId());
            // If this entity is being deleted on the same frame that we're moving, we can have a null entity here.
            if (entity == null)
            {
                continue;
            }

            mass += entity.Mass;
        }

        return mass;
    }

    private void OnGUI()
    {
        //GUI.Label(new Rect(0, 0, 100, 50), $"Total Mass: {TotalMass()}");
    }

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
        lastMovementSendUpdate = Time.time;
    }
}