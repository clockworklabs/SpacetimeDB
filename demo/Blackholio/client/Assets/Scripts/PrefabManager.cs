using SpacetimeDB.Types;
using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class PrefabManager : MonoBehaviour
{
	private static PrefabManager Instance;

	public CircleController CirclePrefab;
	public FoodController FoodPrefab;
	public PlayerController PlayerPrefab;

	private void Awake()
	{
		Instance = this;
	}

	public static CircleController SpawnCircle(Circle circle, PlayerController owner)
	{
		var actor = Instantiate(Instance.CirclePrefab);
		actor.name = $"Circle - {circle.EntityId}";
		actor.Spawn(circle, owner);
		owner.OnCircleSpawned(actor);
		return actor;
	}

	public static FoodController SpawnFood(Food food)
	{
		var actor = Instantiate(Instance.FoodPrefab);
		actor.name = $"Food - {food.EntityId}";
		actor.Spawn(food);
		return actor;
	}

	public static PlayerController SpawnPlayer(Player player)
	{
		var playerController = Instantiate(Instance.PlayerPrefab);
		playerController.name = $"PlayerController - {player.Name}";
		playerController.Initialize(player);
		return playerController;
	}
}
