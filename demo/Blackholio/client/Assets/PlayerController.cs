using System;
using System.Security.Cryptography.X509Certificates;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using Vector2 = SpacetimeDB.Types.Vector2;

public class PlayerController : MonoBehaviour
{
    public int updatesPerSecond = 20;
    public float targetCameraSize = 50;
    public Renderer rend;

    private float? lastMovementSendUpdate;
    private float? previousPositionReceiveUpdateTime;
    private Vector3? previousPosition;
    
    public Identity? identity;
    
    private Vector3? targetPosition;
    private float? targetPositionReceiveUpdateTime;
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

        var entity = Entity.FilterById(playerCircle.EntityId);
        previousPosition = targetPosition = transform.position = new UnityEngine.Vector2
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
                previousPositionReceiveUpdateTime = targetPositionReceiveUpdateTime;
                targetPosition = new UnityEngine.Vector2
                {
                    x = newObj.Position.X,
                    y = newObj.Position.Y,
                };

                var playerRadius = GameManager.MassToRadius(newObj.Mass);
                targetScale = new Vector3
                {
                    x = playerRadius * 2,
                    y = playerRadius * 2,
                    z = playerRadius * 2,
                };

                previousPosition = transform.position;
                targetPositionReceiveUpdateTime = Time.time;
                previousCameraSize = targetCameraSize = playerRadius * 2 + 50.0f;
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

    private void OnGUI()
    {
        if (identity.HasValue && localIdentity == identity.Value)
        {
            var circle = Circle.FilterByCircleId(identity.Value);
            var entity = Entity.FilterById(circle.EntityId);
            GUI.Label(new Rect(0, 0, 100, 50), $"Mass: {entity.Mass}");
        }
    }

    public void Update()
    {
        // Fix interp values
        if (targetPosition.HasValue && targetPositionReceiveUpdateTime.HasValue 
                                    && previousPosition.HasValue && previousPositionReceiveUpdateTime.HasValue)
        {
            transform.position = Vector3.Lerp(previousPosition.Value, 
                targetPosition.Value, (Time.time - targetPositionReceiveUpdateTime.Value) 
                                      / (targetPositionReceiveUpdateTime.Value - previousPositionReceiveUpdateTime.Value));
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
}