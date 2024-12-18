using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.Types;
using Unity.VisualScripting;
using UnityEngine;

public class LeaderboardController : MonoBehaviour
{
    public LeaderboardRow rowPrefab;
    public Transform elementsHierarchy;
    public int rowCount = 10;

    private List<LeaderboardRow> rows = new List<LeaderboardRow>();

    private void Start()
    {
        for (var x = 0; x < rowCount; x++)
        {
            var go = Instantiate(rowPrefab, elementsHierarchy, true);
            rows.Add(go);
        }
    }

    void UpdateRowEnabled(int count)
    {
        for (var x = 0; x < rowCount; x++)
        {
            rows[x].gameObject.SetActive(x < count);
        }
    }
    
    private void Update()
    {
        var players = GameManager.playerIdToPlayerController.Values.Select(
            a => (a, a.TotalMass())).OrderByDescending(a => a.Item2).Take(10).ToList();
        if (PlayerController.Local != null && !players.Any(p => p.a == PlayerController.Local))
        {
            players.Add((PlayerController.Local, PlayerController.Local.TotalMass()));
		}

        var index = 0;
        foreach(var player in players)
        {
            var row = rows[index];
            row.usernameText.text = player.a.GetUsername();
            row.massText.text = player.Item2 + "";
            index++;
        }
        UpdateRowEnabled(index);
    }
}
