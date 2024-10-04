#if UNITY_5_3_OR_NEWER
using System;
using System.Collections.Generic;
using SpacetimeDB;
using UnityEngine;

namespace SpacetimeDB
{
	// This class is only used in Unity projects.
	// Attach this to a gameobject in your scene to use SpacetimeDB.
	public class SpacetimeDBNetworkManager : MonoBehaviour
	{
		private static bool _alreadyInitialized;

		public void Awake()
		{
			// Ensure that users don't create several SpacetimeDBNetworkManager instances.
			// We're using a global (static) list of active connections and we don't want several instances to walk over it several times.
			if (_alreadyInitialized)
			{
				throw new InvalidOperationException("SpacetimeDBNetworkManager is a singleton and should only be attached once.");
			}
			else
			{
				_alreadyInitialized = true;
			}
		}

		internal static HashSet<IDbConnection> ActiveConnections = new();

		private void ForEachConnection(Action<IDbConnection> action)
		{
			foreach (var conn in ActiveConnections)
			{
				action(conn);
			}
		}

		private void Update() => ForEachConnection(conn => conn.FrameTick());
		private void OnDestroy() => ForEachConnection(conn => conn.Disconnect());
	}
}
#endif
