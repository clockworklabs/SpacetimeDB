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
        ConnectionManager.Conn.Db.Player.OnInsert += (ctx, newPlayer) =>
        {
            if (newPlayer.Identity == ConnectionManager.LocalIdentity)
            {
                // We have a player
               gameObject.SetActive(false); 
            }
        };
    }

    public void PlayPressed()
    {
        if (!PlayButton.interactable)
        {
            return;
		}

		Debug.Log("Creating player");
		PlayButton.interactable = false;

        string name = UsernameInputField.text.Trim();
        if (string.IsNullOrEmpty(name))
        {
            name = "<No Name>";
        }
		ConnectionManager.Conn.Reducers.CreatePlayer(name);
    }
}
