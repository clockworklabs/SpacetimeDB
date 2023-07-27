using System.Collections;
using System.Collections.Generic;

namespace SpacetimeDB
{    
    public partial class ReducerEventBase
    {
        public string ReducerName { get; private set; }
        public ulong Timestamp { get; private set; }
        public SpacetimeDB.Identity Identity { get; private set; }
        public string ErrMessage { get; private set; }
        public ClientApi.Event.Types.Status Status { get; private set; }
        protected object Args;

        public ReducerEventBase(string reducerName, ulong timestamp, SpacetimeDB.Identity identity, string errMessage, ClientApi.Event.Types.Status status, object args)
        {
            ReducerName = reducerName;
            Timestamp = timestamp;
            Identity = identity;
            ErrMessage = errMessage;
            Status = status;
            Args = args;
        }
    }
}