using SpacetimeDB.Types;
using System;
using System.Collections.Generic;
using System.Linq;
using UnityEngine;

public static class EntityManager
{
	public static Dictionary<uint, EntityActor> Actors = new Dictionary<uint, EntityActor>();
	public static Dictionary<uint, PlayerController> Players = new Dictionary<uint, PlayerController>();

	public static void Initialize(DbConnection conn)
	{
	}

}