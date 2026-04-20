using Godot;
using SpacetimeDB;
using System;

[GlobalClass]
public partial class SpacetimeDBConnectionManager : Node
{
	// [Signal]
	// public delegate void OnConnectedEventHandler(SpacetimeDBConnectionManager conn);
	// public event Action<IDbConnection, Identity, string> OnConnectedTyped;
	// [Signal]
	// public delegate void OnConnectionErrorEventHandler(string message);
	// public event Action<Exception> OnConnectionErrorTyped;
	// [Signal]
	// public delegate void OnDisconnectedEventHandler(string message);
	// public event Action<IDbConnection, Exception> OnDisconnectedTyped;
	//
	// public IDbConnection Connection { get; private set; }
	// public bool IsActive => Connection?.IsActive == true;
	// private string _authToken;
	// public string AuthToken
	// {
	// 	get => Connection != null ? _authToken : null;
	// 	private set => _authToken = value;
	// }
	// public Identity? Identity => Connection?.Identity;
	//
	// public override void _ExitTree()
	// {
	// 	DisconnectFromDatabase();
	// }
	//
	public void ConnectToDatabase(string host, string databaseName, string authTokenKey)
	{
		// if (Connection?.IsActive == true)
		// {
		// 	GD.PrintErr("SpacetimeDB connection is already active.");
		// 	return;
		// }
		//
		// SpacetimeDB.AuthToken.Init(authTokenKey); // Not sure about this
		//
		// GD.Print($"Connecting to SpacetimeDB at {host} / {databaseName}");
		//
		// Connection = DbConnection.Builder()
		// 	.WithUri(host)
		// 	.WithDatabaseName(databaseName)
		// 	.WithToken(SpacetimeDB.AuthToken.Token)
		// 	.OnConnect(OnConnect)
		// 	.OnConnectError(OnConnectError)
		// 	.OnDisconnect(OnDisconnect)
		// 	.Build();
	}
	//
	// public void DisconnectFromDatabase()
	// {
	// 	if (Connection?.IsActive == true)
	// 	{
	// 		Connection.Disconnect();
	// 	}
	// }
	//
	// public override void _Process(double delta)
	// {
	// 	Connection?.FrameTick();
	// }
	//
	// private void OnConnect(DbConnection conn, Identity identity, string authToken)
	// {
	// 	SpacetimeDB.AuthToken.SaveToken(authToken);
	//
	// 	OnConnectedTyped?.Invoke(conn, identity, authToken);
	// 	EmitSignal(SignalName.OnConnected, identity.ToString());
	// }
	//
	// private void OnConnectError(Exception exception)
	// {
	// 	OnConnectionErrorTyped?.Invoke(exception);
	// 	EmitSignal(SignalName.OnConnectionError, exception.ToString());
	// }
	//
	// private void OnDisconnect(DbConnection conn, Exception exception)
	// {
	// 	OnDisconnectedTyped?.Invoke(conn, exception);
	// 	EmitSignal(SignalName.OnDisconnected, exception?.ToString() ?? string.Empty);
	// 	Connection = null;
	// }
}
