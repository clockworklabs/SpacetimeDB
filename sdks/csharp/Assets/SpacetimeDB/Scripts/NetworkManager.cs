using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using ClientApi;
using SpacetimeDB;
using UnityEngine;

namespace SpacetimeDB
{
    public class NetworkManager : MonoBehaviour
    {
        public enum TableOp
        {
            Insert,
            Delete,
            Update
        }

        [Serializable]
        public class Message
        {
            public string fn;
            public object[] args;
        }

        private struct DbEvent
        {
            public Type clientTableType;
            public TableOp op;
            public object oldValue;
            public object newValue;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue);

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?> onConnectError;
        
        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception> onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?> onDisconnect;

        /// <summary>
        /// Invoked on each row update to each table.
        /// </summary>
        public event RowUpdate onRowUpdate;

        /// <summary>
        /// Callback is invoked after a transaction or subscription update is received and all updates have been applied.
        /// </summary>
        public event Action onTransactionComplete;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<Hash> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB;
        public static Dictionary<string, MethodInfo> reducerEventCache = new Dictionary<string, MethodInfo>();

        private Thread messageProcessThread;

        public static NetworkManager instance;

        public string TokenKey { get { return GetTokenKey(); } }

        protected void Awake()
        {
            if (instance != null)
            {
                Debug.LogError($"There is more than one {GetType()}");
                return;
            }

            instance = this;

            var options = new SpacetimeDB.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new SpacetimeDB.WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnClose += (code, error) => onDisconnect?.Invoke(code, error);
            webSocket.OnConnect += () => onConnect?.Invoke();
            webSocket.OnConnectError += a => onConnectError?.Invoke(a);
            webSocket.OnSendError += a => onSendError?.Invoke(a);
            
            clientDB = new ClientCache();

            var type = typeof(IDatabaseTable);
            var types = AppDomain.CurrentDomain.GetAssemblies()
                .SelectMany(s => s.GetTypes())
                .Where(p => type.IsAssignableFrom(p));
            foreach (var @class in types)
            {
                if (!@class.IsClass)
                {
                    continue;
                }

                var typeDefFunc = @class.GetMethod("GetTypeDef", BindingFlags.Static | BindingFlags.Public);
                var typeDef = typeDefFunc!.Invoke(null, null) as TypeDef;
                // var conversionFunc = @class.GetMethod("op_Explicit");
                var conversionFunc = @class.GetMethods().FirstOrDefault(a =>
                    a.Name == "op_Explicit" && a.GetParameters().Length > 0 &&
                    a.GetParameters()[0].ParameterType == typeof(TypeValue));
                clientDB.AddTable(@class, typeDef,
                    a => { return conversionFunc!.Invoke(null, new object[] { a }); });
            }

            // cache all our reducer events by their function name 
            foreach (var methodInfo in (typeof(Reducer)).GetMethods())
            {
                var ca = methodInfo.GetCustomAttribute<ReducerEvent>();
                if (ca != null)
                {
                    ReducerEvent reducerEvent = (ReducerEvent)ca;
                    reducerEventCache.Add(reducerEvent.FunctionName, methodInfo);
                }
            }

            messageProcessThread = new Thread(ProcessMessages);
            messageProcessThread.Start();
        }

        private readonly BlockingCollection<byte[]> _messageQueue = new BlockingCollection<byte[]>();
        private readonly ConcurrentQueue<byte[]> _completedMessages = new ConcurrentQueue<byte[]>();

        void ProcessMessages()
        {
            while (true)
            {
                var bytes = _messageQueue.Take();

                var message = ClientApi.Message.Parser.ParseFrom(bytes);
                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        break;
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        break;
                }

                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        // First apply all of the state
                        System.Diagnostics.Debug.Assert(subscriptionUpdate != null,
                            nameof(subscriptionUpdate) + " != null");
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            foreach (var row in update.TableRowOperations)
                            {
                                var table = clientDB.GetTable(update.TableName);
                                var typeDef = table.RowSchema;
                                var (typeValue, _) = TypeValue.Decode(typeDef, row.Row);
                                if (typeValue.HasValue)
                                {
                                    // Here we are decoding on our message thread so that by the time we get to the
                                    // main thread the cache is already warm.
                                    table.Decode(row.RowPk.ToByteArray(), typeValue.Value);
                                }
                            }
                        }

                        break;
                }

                _completedMessages.Enqueue(bytes);
            }
        }

        private void OnDestroy()
        {
            connectionClosed = true;
            webSocket.Close();
            webSocket = null;
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="host">The host or IP address and the port to connect to. Example: spacetime.spacetimedb.net:3000</param>
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        public void Connect(string host, string addressOrName)
        {
            var token = PlayerPrefs.HasKey(GetTokenKey()) ? PlayerPrefs.GetString(GetTokenKey()) : null;

            Task.Run(async () =>
            {
                try
                {
                    await webSocket.Connect(token, host, addressOrName);
                }
                catch (Exception e)
                {
                    if (connectionClosed)
                    {
                        Debug.Log("Connection closed gracefully.");
                        return;
                    }

                    Debug.LogException(e);
                }
            });
        }

        readonly List<DbEvent> _dbEvents = new List<DbEvent>();

        private void OnMessageProcessComplete(byte[] bytes)
        {
            _dbEvents.Clear();
            var message = ClientApi.Message.Parser.ParseFrom(bytes);

            SubscriptionUpdate subscriptionUpdate = null;
            switch (message.TypeCase)
            {
                case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    subscriptionUpdate = message.SubscriptionUpdate;
                    break;
                case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                    subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                    break;
            }

            switch (message.TypeCase)
            {
                case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                    // First apply all of the state
                    foreach (var update in subscriptionUpdate.TableUpdates)
                    {
                        var tableName = update.TableName;
                        var table = clientDB.GetTable(tableName);
                        if (table == null)
                        {
                            continue;
                        }

                        foreach (var row in update.TableRowOperations)
                        {
                            var rowPk = row.RowPk.ToByteArray();

                            switch (row.Op)
                            {
                                case TableRowOperation.Types.OperationType.Delete:
                                    var deletedValue = table.Delete(rowPk);
                                    if (deletedValue != null)
                                    {
                                        _dbEvents.Add(new DbEvent
                                        {
                                            clientTableType = table.ClientTableType,
                                            op = TableOp.Delete,
                                            newValue = null,
                                            oldValue = deletedValue,
                                        });
                                    }

                                    break;
                                case TableRowOperation.Types.OperationType.Insert:
                                    var insertedValue = table.Insert(rowPk);
                                    if (insertedValue != null)
                                    {
                                        _dbEvents.Add(new DbEvent
                                        {
                                            clientTableType = table.ClientTableType,
                                            op = TableOp.Insert,
                                            newValue = insertedValue,
                                            oldValue = null
                                        });
                                    }

                                    break;
                            }
                        }
                    }

                    // Send out events
                    var eventCount = _dbEvents.Count;
                    for (int i = 0; i < eventCount; i++)
                    {
                        string tableName = _dbEvents[i].clientTableType.Name;

                        bool isUpdate = false;
                        if (i < eventCount - 1)
                        {
                            if (_dbEvents[i].op == TableOp.Delete && _dbEvents[i + 1].op == TableOp.Insert)
                            {
                                // somewhat hacky: Delete followed by an insert on the same table is considered an update.
                                isUpdate = tableName.Equals(_dbEvents[i + 1].clientTableType.Name);
                            }
                        }

                        TableOp tableOp = _dbEvents[i].op;

                        object oldValue = _dbEvents[i].oldValue, newValue = _dbEvents[i].newValue;

                        if (isUpdate)
                        {
                            // Merge delete and insert in one update
                            tableOp = TableOp.Update;
                            newValue = _dbEvents[i + 1].newValue;

                            i++;

                            var clientEvent = _dbEvents[i].clientTableType.GetMethod("OnUpdateEvent");
                            if(clientEvent != null)
                            {
                                clientEvent.Invoke(null, new object[] { oldValue, newValue });
                            }                            
                        }
                        else if(tableOp == TableOp.Insert)
                        {
                            var clientEvent = _dbEvents[i].clientTableType.GetMethod("OnInsertEvent");
                            if (clientEvent != null)
                            {
                                clientEvent.Invoke(null, new object[] { newValue });
                            }
                        }
                        else if(tableOp == TableOp.Delete)
                        {
                            var clientEvent = _dbEvents[i].clientTableType.GetMethod("OnDeleteEvent");
                            if (clientEvent != null)
                            {
                                clientEvent.Invoke(null, new object[] { oldValue });
                            }
                        }

                        var clientRowUpdate = _dbEvents[i].clientTableType.GetMethod("OnRowUpdateEvent");
                        if (clientRowUpdate != null)
                        {
                            clientRowUpdate.Invoke(null, new object[] { tableOp, oldValue, newValue });
                        }

                        onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue);
                    }

                    switch (message.TypeCase)
                    {
                        case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                            onTransactionComplete?.Invoke();
                            break;
                        case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                            onTransactionComplete?.Invoke();
                            onEvent?.Invoke(message.TransactionUpdate.Event);

                            string functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.ContainsKey(functionName))
                            {
                                reducerEventCache[functionName].Invoke(null, new object[] { message.TransactionUpdate.Event });
                            }
                            break;
                    }

                    break;
                case ClientApi.Message.TypeOneofCase.IdentityToken:
                    onIdentityReceived?.Invoke(Hash.From(message.IdentityToken.Identity.ToByteArray()));
                    PlayerPrefs.SetString(GetTokenKey(), message.IdentityToken.Token);
                    break;
                case ClientApi.Message.TypeOneofCase.Event:
                    onEvent?.Invoke(message.Event);
                    break;
            }
        }


        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        private string GetTokenKey()
        {
            var key = "spacetimedb.identity_token";
#if UNITY_EDITOR
            // Different editors need different keys
            key += $" - {Application.dataPath}";
#endif
            return key;
        }

        internal void InternalCallReducer(Message message)
        {
            var json = Newtonsoft.Json.JsonConvert.SerializeObject(message);
            webSocket.Send(Encoding.ASCII.GetBytes(json));
        }

        private void Update()
        {
            webSocket.Update();

            while (_completedMessages.TryDequeue(out var result))
            {
                OnMessageProcessComplete(result);
            }
        }
    }
}