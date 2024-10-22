using System;
using System.Collections;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.UI;

public class UIUsernameChooser : MonoBehaviour
{
    public TMPro.TMP_InputField usernameInputField;
    public Button playButton;

    private bool sentCreatePlayer;

    public static UIUsernameChooser instance;
    
    private void Start()
    {
        instance = this;
        GameManager.conn.Db.Player.OnInsert += (ctx, newPlayer) =>
        {
            if (newPlayer.Identity == GameManager.localIdentity)
            {
                // We have a player
               gameObject.SetActive(false); 
            }
        };
    }

    public void PlayPressed()
    {
        Debug.Log("Play pressed");
        if (sentCreatePlayer)
        {
            return;
        }
        Debug.Log("Creating player");

        sentCreatePlayer = true;
        GameManager.conn.Reducers.CreatePlayer(usernameInputField.text);
        playButton.interactable = false;
    }
}
