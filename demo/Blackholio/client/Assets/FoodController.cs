using System;
using System.Collections;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using Unity.VisualScripting;
using UnityEngine;
using Random = System.Random;

public class FoodController : MonoBehaviour
{
    [DoNotSerialize] public uint entityId;
    public Renderer rend;

    public void Spawn(uint entityId)
    {
        this.entityId = entityId;
        Food.OnRowUpdate += FoodOnRowUpdate;

        var entity = Entity.FilterById(entityId);
        var position = new UnityEngine.Vector2
        {
            x = entity.Position.X,
            y = entity.Position.Y,
        };
        transform.localScale = new Vector3
        {
            x = entity.Radius * 2,
            y = entity.Radius * 2,
            z = entity.Radius * 2,
        };
        transform.position = position;
        rend.material.color = GameManager.GetRandomColor(entity.Id);
    }

    private void OnDestroy()
    {
        Food.OnRowUpdate -= FoodOnRowUpdate;
    }

    private void FoodOnRowUpdate(SpacetimeDBClient.TableOp op, Food oldvalue, Food newvalue, ReducerEvent dbevent)
    {
        switch (op)
        {
            case SpacetimeDBClient.TableOp.Delete:
                if (oldvalue.EntityId == entityId)
                {
                    Destroy(gameObject);
                }

                break;
        }
    }
}