using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class ConnectionManager : MonoBehaviour
{
    const string SERVER_URL = "http://127.0.0.1:3000";
    const string MODULE_NAME = "untitled-circle-game";

    public static event Action OnConnected;
    public static event Action OnSubscriptionApplied;

	public static ConnectionManager Instance { get; private set; }
    public static Identity LocalIdentity { get; private set; }
    public static DbConnection Conn { get; private set; }



    private void Start()
    {
        Instance = this;
        Application.targetFrameRate = 60;

        // Now that we’ve registered all our callbacks, lets connect to spacetimedb
        var builder = DbConnection.Builder().OnConnect((_conn, identity, token) => {
            // Called when we connect to SpacetimeDB and receive our client identity
            Debug.Log("Connected.");
            AuthToken.SaveToken(token);
            LocalIdentity = identity;

			EntityManager.Initialize(Conn);
            OnConnected?.Invoke();

            // Request all tables
            Conn.SubscriptionBuilder().OnApplied(ctx =>
            {
                Debug.Log("Subscription applied!");
                OnSubscriptionApplied?.Invoke();
            }).Subscribe("SELECT * FROM *");
        }).OnConnectError((ex) =>
        {
            // Called when we have an error connecting to SpacetimeDB
            Debug.LogError($"Connection error: {ex}");
        }).OnDisconnect((_conn, ex) =>
        {
            // Called when we are disconnected from SpacetimeDB
            Debug.Log("Disconnected.");
            if (ex != null)
            {
                Debug.LogException(ex);
            }
        }).WithUri(SERVER_URL)
            .WithModuleName(MODULE_NAME);
		if (PlayerPrefs.HasKey(AuthToken.GetTokenKey()))
        {
			builder = builder.WithCredentials((default, AuthToken.Token));
        }
        Conn = builder.Build();

#pragma warning disable CS0612 // Type or member is obsolete
		Conn.onUnhandledReducerError += InstanceOnUnhandledReducerError;
#pragma warning restore CS0612 // Type or member is obsolete
    }

    private void InstanceOnUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
    {
        Debug.LogError($"There was an error!\r\n{(reducerEvent.Status as Status.Failed)?.Failed_}");
    }

    public void Disconnect()
    {
        Conn.Disconnect();
        Conn = null;
    }

    public static bool IsConnected()
    {
        return Conn != null && Conn.IsActive;
    }
}
