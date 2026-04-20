#if GODOT
using Godot;

namespace SpaceTimeDB
{
	[GlobalClass]
	public partial class STDBSimpleConnection : SpacetimeDBConnectionManager
	{
		[Export]
		public string Host { get; set; } = "http://localhost:3000";
		[Export]
		public string DatabaseName { get; set; } = "quickstart-chat-t8oj3";
		[Export]
		public string AuthTokenKey { get; set; } = ".spacetime_csharp_quickstart";
		[Export]
		public bool ConnectOnReady { get; set; } = true;

		public override void _Ready()
		{
			if (ConnectOnReady)
			{
				ConnectToDatabase();
			}
		}

		public void ConnectToDatabase() => ConnectToDatabase(Host, DatabaseName, AuthTokenKey);
	}
}
#endif
