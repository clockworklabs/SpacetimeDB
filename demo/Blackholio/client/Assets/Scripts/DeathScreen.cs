using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class DeathScreen : MonoBehaviour
{
	public static DeathScreen Instance { get; private set; }

	public Button RespawnButton;

	public void Awake()
	{
		Instance = this;
	}

	public void SetVisible(bool visible)
	{
		gameObject.SetActive(visible);
	}

	public void Respawn()
	{
		GameManager.Conn.Reducers.Respawn();
		SetVisible(false);
	}
}
