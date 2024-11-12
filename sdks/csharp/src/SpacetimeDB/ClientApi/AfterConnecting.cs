#nullable enable

using System;
using SpacetimeDB;
using System.Collections.Generic;
using System.Runtime.Serialization;

namespace SpacetimeDB.ClientApi
{
	[SpacetimeDB.Type]
	[DataContract]
	public partial class AfterConnecting
	{
		[DataMember(Name = "identity_token")]
		public SpacetimeDB.ClientApi.IdentityToken IdentityToken;
		[DataMember(Name = "ids_to_names")]
		public SpacetimeDB.ClientApi.IdsToNames IdsToNames;

		public AfterConnecting(
			SpacetimeDB.ClientApi.IdentityToken IdentityToken,
			SpacetimeDB.ClientApi.IdsToNames IdsToNames
		)
		{
			this.IdentityToken = IdentityToken;
			this.IdsToNames = IdsToNames;
		}

		public AfterConnecting()
		{
			this.IdentityToken = new();
			this.IdsToNames = new();
		}

	}
}
