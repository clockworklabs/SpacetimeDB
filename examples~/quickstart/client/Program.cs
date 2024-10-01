using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Net.WebSockets;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "chatqs";

// our local client SpacetimeDB identity
Identity? local_identity = null;
// declare a thread safe queue to store commands
var input_queue = new ConcurrentQueue<(string Command, string Args)>();

void Main()
{
    AuthToken.Init(".spacetime_csharp_quickstart");

    // TODO: just do `var conn = DbConnection...` when OnConnect signature is fixed.
    DbConnection? conn = null;

    conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DBNAME)
        //.WithCredentials((null, AuthToken.Token))
        .OnConnect(OnConnect)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnect)
        .Build();

    conn.RemoteTables.User.OnInsert += User_OnInsert;
    conn.RemoteTables.User.OnUpdate += User_OnUpdate;

    conn.RemoteTables.Message.OnInsert += Message_OnInsert;

    conn.RemoteReducers.OnSetName += Reducer_OnSetNameEvent;
    conn.RemoteReducers.OnSendMessage += Reducer_OnSendMessageEvent;

#pragma warning disable CS0612 // Using obsolete API
    conn.onUnhandledReducerError += onUnhandledReducerError;
#pragma warning restore CS0612 // Using obsolete API

    // declare a threadsafe cancel token to cancel the process loop
    var cancellationTokenSource = new CancellationTokenSource();

    // spawn a thread to call process updates and process commands
    var thread = new Thread(() => ProcessThread(conn, cancellationTokenSource.Token));
    thread.Start();

    InputLoop();

    // this signals the ProcessThread to stop
    cancellationTokenSource.Cancel();
    thread.Join();
}

string UserNameOrIdentity(User user) => user.Name ?? user.Identity.ToString()[..8];

void User_OnInsert(EventContext ctx, User insertedValue)
{
    if (insertedValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(insertedValue)} is online");
    }
}

void User_OnUpdate(EventContext ctx, User oldValue, User newValue)
{
    if (oldValue.Name != newValue.Name)
    {
        Console.WriteLine($"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}");
    }
    if (oldValue.Online != newValue.Online)
    {
        if (newValue.Online)
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} connected.");
        }
        else
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} disconnected.");
        }
    }
}

void PrintMessage(RemoteTables tables, Message message)
{
    var sender = tables.User.FindByIdentity(message.Sender);
    var senderName = "unknown";
    if (sender != null)
    {
        senderName = UserNameOrIdentity(sender);
    }

    Console.WriteLine($"{senderName}: {message.Text}");
}

void Message_OnInsert(EventContext ctx, Message insertedValue)
{
    if (ctx.Reducer is not Event<Reducer>.SubscribeApplied)
    {
        PrintMessage(ctx.Db, insertedValue);
    }
}

void Reducer_OnSetNameEvent(EventContext ctx, string name)
{
    if (ctx.Reducer is Event<Reducer>.Reducer reducer)
    {
        var e = reducer.ReducerEvent;
        if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
        {
            Console.Write($"Failed to change name to {name}: {error}");
        }
    }
}

void Reducer_OnSendMessageEvent(EventContext ctx, string text)
{
    if (ctx.Reducer is Event<Reducer>.Reducer reducer)
    {
        var e = reducer.ReducerEvent;
        if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
        {
            Console.Write($"Failed to send message {text}: {error}");
        }
    }
}

void OnConnect(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);

    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .Subscribe("SELECT * FROM User");

    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .Subscribe("SELECT * FROM Message");
}

void OnConnectError(WebSocketError? error, string message)
{

}

void OnDisconnect(DbConnection conn, WebSocketCloseStatus? status, WebSocketError? error)
{

}

void PrintMessagesInOrder(RemoteTables tables)
{
    foreach (Message message in tables.Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(tables, message);
    }
}

void OnSubscriptionApplied(EventContext ctx)
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder(ctx.Db);
}

void onUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
{
    Console.WriteLine($"Unhandled reducer error in {reducerEvent.Reducer}: {reducerEvent.Status}");
}

void ProcessThread(DbConnection conn, CancellationToken ct)
{
    try
    {
        // loop until cancellation token
        while (!ct.IsCancellationRequested)
        {
            conn.FrameTick();

            ProcessCommands(conn.RemoteReducers);

            Thread.Sleep(100);
        }
    }
    finally
    {
        conn.Disconnect();
    }
}

void InputLoop()
{
    while (true)
    {
        var input = Console.ReadLine();
        if (input == null)
        {
            break;
        }

        if (input.StartsWith("/name "))
        {
            input_queue.Enqueue(("name", input[6..]));
            continue;
        }
        else
        {
            input_queue.Enqueue(("message", input));
        }
    }
}

void ProcessCommands(RemoteReducers reducers)
{
    // process input queue commands
    while (input_queue.TryDequeue(out var command))
    {
        switch (command.Command)
        {
            case "message":
                reducers.SendMessage(command.Args);
                break;
            case "name":
                reducers.SetName(command.Args);
                break;
        }
    }
}

Main();
