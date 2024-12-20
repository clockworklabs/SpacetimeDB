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
		conn.Db.Circle.OnInsert += CircleOnInsert;
		conn.Db.Entity.OnUpdate += EntityOnUpdate;
		conn.Db.Entity.OnDelete += EntityOnDelete;
		conn.Db.Food.OnInsert += FoodOnInsert;
		conn.Db.Player.OnInsert += PlayerOnInsert;
		conn.Db.Player.OnDelete += PlayerOnDelete;
	}



	private static void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
	{
		if (!Actors.TryGetValue(newEntity.EntityId, out var actor))
		{
			return;
		}
		actor.OnEntityUpdated(newEntity);
	}

	private static void EntityOnDelete(EventContext context, Entity oldEntity)
	{
		if (Actors.Remove(oldEntity.EntityId, out var actor))
		{
			actor.OnDelete();
		}
	}



	private static void CircleOnInsert(EventContext context, Circle insertedValue)
	{
		var player = GetOrCreatePlayer(insertedValue.PlayerId);
		var actor = PrefabManager.SpawnCircle(insertedValue, player);
		Actors.Add(insertedValue.EntityId, actor);
	}

	private static void FoodOnInsert(EventContext context, Food insertedValue)
	{
		var actor = PrefabManager.SpawnFood(insertedValue);
		Actors.Add(insertedValue.EntityId, actor);
	}



	private static void PlayerOnInsert(EventContext context, Player insertedPlayer)
	{
		GetOrCreatePlayer(insertedPlayer.PlayerId);
	}

	private static void PlayerOnDelete(EventContext context, Player deletedvalue)
	{
		if (Players.Remove(deletedvalue.PlayerId, out var playerController))
		{
			GameObject.Destroy(playerController.gameObject);
		}
	}

	private static PlayerController GetOrCreatePlayer(uint playerId)
	{
		if (!Players.TryGetValue(playerId, out var playerController))
		{
			var player = ConnectionManager.Conn.Db.Player.PlayerId.Find(playerId);
			playerController = PrefabManager.SpawnPlayer(player);
			Players.Add(playerId, playerController);
		}

		return playerController;
	}
}