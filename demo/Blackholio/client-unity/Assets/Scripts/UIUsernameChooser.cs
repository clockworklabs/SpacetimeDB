using System;
using System.Collections.Generic;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.UI;

public class UIUsernameChooser : MonoBehaviour
{
    public static UIUsernameChooser Instance { get; private set; }

    // The elements that compose the username chooser UI
    public GameObject[] elements;
    public TMPro.TMP_InputField UsernameInputField;

    private void Start()
    {
        Instance = this;
    }

    public void PlayPressed()
    {
        Debug.Log("Creating player");

        var name = UsernameInputField.text.Trim();
        if (string.IsNullOrEmpty(name))
        {
            name = "<No Name>";
        }
        GameManager.Conn.Reducers.EnterGame(name);
        Show(false);
    }

    public void Show(bool showing)
    {
        foreach (var element in elements)
        {
            element.SetActive(showing);
        }
    }
}
