using System;
using System.Collections;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.Assertions.Must;
using Event = ClientApi.Event;
using Vector2 = SpacetimeDB.Types.Vector2;

public class PlayerController : MonoBehaviour
{
    public int updatesPerSecond = 20;
    public float targetCameraSize = 50;
    public Renderer rend;

    private float? lastMovementUpdate;
    
    public Identity? identity;
    
    private Vector3? previousPosition;
    private Vector3? targetPosition;
    private Vector3? targetScale;
    private float? previousCameraSize;
    private float? lastRowUpdate;

    public static Identity localIdentity;
    public static PlayerController Local;
    
    public void Spawn(Identity identity, Circle playerCircle)
    {
        this.identity = identity;
        Entity.OnRowUpdate += OnRowOp;

        if (localIdentity == identity)
        {
            Local = this;
        }

        var circlePosition = Entity.FilterById(playerCircle.EntityId);
        previousPosition = targetPosition = transform.position = new UnityEngine.Vector2
        {
            x = circlePosition.Position.X,
            y = circlePosition.Position.Y,
        };
                     
        targetScale = transform.localScale = new Vector3
        {
            x = circlePosition.Radius * 2,
            y = circlePosition.Radius * 2,
            z = circlePosition.Radius * 2,
        };
        rend.material.color = GameManager.GetRandomColor(circlePosition.Id);
    }

    public void OnDestroy()
    {
        Entity.OnRowUpdate -= OnRowOp;
    }

    public void OnRowOp(SpacetimeDBClient.TableOp op, Entity oldObj, Entity newObj, ReducerEvent e)
    {
        switch (op)
        {
            case SpacetimeDBClient.TableOp.Update:
                // Make sure this is a circle
                var newCircle = Circle.FilterByEntityId(newObj.Id);
                if (identity.HasValue && newCircle == null || newCircle.CircleId != identity)
                {
                    return;
                }
               
                previousPosition = targetPosition;
                if (targetPosition.HasValue)
                {
                    transform.position = targetPosition.Value;
                }
                targetPosition = new UnityEngine.Vector2
                {
                    x = newObj.Position.X,
                    y = newObj.Position.Y,
                };
                
                targetScale = new Vector3
                {
                    x = newObj.Radius * 2,
                    y = newObj.Radius * 2,
                    z = newObj.Radius * 2,
                };

                previousCameraSize = targetCameraSize;
                targetCameraSize = newObj.Radius * 2 + 50.0f;
                // For next stream, we should track updates per thing so that we aren't interrupting animations
                lastUpdateTime = Time.time;
                break;
            case SpacetimeDBClient.TableOp.Delete:
                var oldCircle = Circle.FilterByEntityId(oldObj.Id);
                if (identity.HasValue && oldCircle == null || oldCircle.CircleId != identity)
                {
                    return;
                }
                Destroy(gameObject);
                break;
        }
    }

    public void Update()
    {
        // Fix interp values
        var interpValue = Time.time - pre / updatesPerSecond;
        if (targetPosition.HasValue && previousPosition.HasValue)
        {
            transform.position = Vector3.Lerp(previousPosition.Value, 
                targetPosition.Value, interpValue);
        }

        if (targetScale.HasValue)
        {
            transform.localScale = Vector3.MoveTowards(transform.localScale,
                targetScale.Value, 0.2f);
        }

        if (localIdentity == identity && previousCameraSize.HasValue)
        {
            GameManager.localCamera.orthographicSize =
                Mathf.Lerp(previousCameraSize.Value, targetCameraSize, interpValue / 10);
        }
        
        if (!identity.HasValue || localIdentity != identity.Value || 
            (lastMovementUpdate.HasValue && Time.time - lastMovementUpdate.Value < 1.0f / updatesPerSecond))
        {
            return;
        }

        lastMovementUpdate = Time.time;
        
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
}