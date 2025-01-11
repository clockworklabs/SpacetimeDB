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
		var entityController = Instantiate(Instance.CirclePrefab);
		entityController.name = $"Circle - {circle.EntityId}";
		entityController.Spawn(circle, owner);
		owner.OnCircleSpawned(entityController);
		return entityController;
	}

	public static FoodController SpawnFood(Food food)
	{
		var entityController = Instantiate(Instance.FoodPrefab);
		entityController.name = $"Food - {food.EntityId}";
		entityController.Spawn(food);
		return entityController;
	}

	public static PlayerController SpawnPlayer(Player player)
	{
		var playerController = Instantiate(Instance.PlayerPrefab);
		playerController.name = $"PlayerController - {player.Name}";
		playerController.Initialize(player);
		return playerController;
	}
}
