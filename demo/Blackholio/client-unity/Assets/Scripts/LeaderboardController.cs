using System.Collections.Generic;
using System.Linq;
using UnityEngine;

public class LeaderboardController : MonoBehaviour
{
    const int MAX_ROW_COUNT = 11; //10 + local player

    public LeaderboardRow RowPrefab;
    public Transform Root;

    private LeaderboardRow[] Rows = new LeaderboardRow[MAX_ROW_COUNT];

    private void Start()
    {
        for (var i = 0; i < MAX_ROW_COUNT; i++)
        {
            var go = Instantiate(RowPrefab, Root, true);
            go.gameObject.SetActive(false);
            Rows[i] = go;
        }
    }
    
    private void Update()
    {
        var players = GameManager.Players.Values
            .Select(a => (player: a, mass: a.TotalMass()))
            .Where(a => a.mass > 0)
            .OrderByDescending(a => a.mass)
            .Take(10)
            .ToList();
        var localPlayer = PlayerController.Local;
		if (localPlayer != null && !players.Any(p => p.player == localPlayer) && localPlayer.NumberOfOwnedCircles > 0)
        {
            players.Add((localPlayer, localPlayer.TotalMass()));
		}

        int i;
        for (i = 0; i < players.Count; i++)
		{
            var player = players[i];
			var row = Rows[i];
            row.SetData(player.player.Username, player.mass);
            row.gameObject.SetActive(true);
		}
        for (; i < MAX_ROW_COUNT; i++)
		{
			Rows[i].gameObject.SetActive(false);
		}
    }
}
