using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class ArenaController : MonoBehaviour
{
    public float thickness = 10;

    private void Start()
    {
        GameManager.conn.Db.Config.OnInsert += (ctx, value) =>
        {
            var worldSize = value.WorldSize;
        };

    }
}
