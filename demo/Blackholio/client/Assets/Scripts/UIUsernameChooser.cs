using System;
using System.Collections.Generic;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.UI;

public class UIUsernameChooser : MonoBehaviour
{
    public static UIUsernameChooser Instance { get; private set; }

    public TMPro.TMP_InputField UsernameInputField;
    public Button PlayButton;
    
    private void Start()
    {
        Instance = this;
        GameManager.Conn.Db.Player.OnInsert += (ctx, newPlayer) =>
        {
            if (newPlayer.Identity == GameManager.LocalIdentity)
            {
                // We have a player
                UsernameInputField.text = newPlayer.Name;
			}
        };
    }

    public void PlayPressed()
    {
		Debug.Log("Creating player");

        string name = UsernameInputField.text.Trim();
        if (string.IsNullOrEmpty(name))
        {
            name = "<No Name>";
        }
		GameManager.Conn.Reducers.EnterGame(name);
		gameObject.SetActive(false);
	}
}
