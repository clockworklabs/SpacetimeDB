using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class ArenaController : MonoBehaviour
{
    public SpriteRenderer backgroundPrefab;
    public float thickness = 10;

    private SpriteRenderer backgroundInstance;

    private void Start()
    {
        GameManager.conn.Db.Config.OnInsert += (ctx, value) =>
        {
            var worldSize = value.WorldSize;
            var north = GameObject.CreatePrimitive(PrimitiveType.Cube);
            north.transform.localScale = new Vector3(worldSize + thickness * 2.0f, thickness, 1);
            north.transform.position = new Vector3(worldSize / 2.0f, worldSize + thickness / 2, 1);
            var south = GameObject.CreatePrimitive(PrimitiveType.Cube);
            south.transform.localScale = new Vector3(worldSize + thickness * 2.0f, thickness, 1);
            south.transform.position = new Vector3(worldSize / 2.0f, -thickness / 2, 1);
            var east = GameObject.CreatePrimitive(PrimitiveType.Cube);
            east.transform.localScale = new Vector3(thickness, worldSize + thickness * 2.0f, 1);
            east.transform.position = new Vector3(worldSize + thickness / 2, worldSize / 2.0f, 1);
            var west = GameObject.CreatePrimitive(PrimitiveType.Cube);
            west.transform.localScale = new Vector3(thickness, worldSize + thickness * 2.0f, 1);
            west.transform.position = new Vector3(-thickness / 2, worldSize / 2.0f, 1);

            backgroundInstance = Instantiate(backgroundPrefab);
            var size = worldSize / backgroundInstance.transform.localScale.x;
            backgroundInstance.size = new UnityEngine.Vector2(size, size);
            backgroundInstance.transform.position = new Vector3((float)worldSize / 2, (float)worldSize / 2);
        };

    }
}
