using System.Collections;
using System.Collections.Generic;
using TMPro;
using UnityEngine;

public class LeaderboardRow : MonoBehaviour
{
    public TextMeshProUGUI UsernameText;
    public TextMeshProUGUI MassText;

    public void SetData(string username, int mass)
	{
		UsernameText.text = username;
		MassText.text = mass.ToString();
	}
}
