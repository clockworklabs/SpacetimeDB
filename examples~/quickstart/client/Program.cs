using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Net.WebSockets;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.ClientApi;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "chatqs";

DbConnection? conn = null;

// our local client SpacetimeDB identity
Identity? local_identity = null;
// declare a thread safe queue to store commands
var input_queue = new ConcurrentQueue<(string Command, string Args)>();
// declare a threadsafe cancel token to cancel the process loop
var cancel_token = new CancellationTokenSource();

void Main()
{
    AuthToken.Init(".spacetime_csharp_quickstart");

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

    conn.onSubscriptionApplied += OnSubscriptionApplied;
    conn.onUnhandledReducerError += onUnhandledReducerError;

    // spawn a thread to call process updates and process commands
    var thread = new Thread(ProcessThread);
    thread.Start();

    InputLoop();

    // this signals the ProcessThread to stop
    cancel_token.Cancel();
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

void PrintMessage(Message message)
{
    var sender = conn.RemoteTables.User.FindByIdentity(message.Sender);
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
        PrintMessage(insertedValue);
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

void OnConnect(Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);

    conn!.Subscribe(new List<string> { "SELECT * FROM User", "SELECT * FROM Message" });
}

void OnConnectError(WebSocketError? error, string message)
{

}

void OnDisconnect(DbConnection conn, WebSocketCloseStatus? status, WebSocketError? error)
{

}

void PrintMessagesInOrder()
{
    foreach (Message message in conn.RemoteTables.Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(message);
    }
}

void OnSubscriptionApplied()
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder();
}

void onUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
{
    Console.WriteLine($"Unhandled reducer error in {reducerEvent.Reducer}: {reducerEvent.Status}");
}

void ProcessThread()
{
    try
    {
        // loop until cancellation token
        while (!cancel_token.IsCancellationRequested)
        {
            conn.Update();

            ProcessCommands();

            Thread.Sleep(100);
        }
    }
    finally
    {
        conn.Close();
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

void ProcessCommands()
{
    // process input queue commands
    while (input_queue.TryDequeue(out var command))
    {
        switch (command.Command)
        {
            case "message":
                conn.RemoteReducers.SendMessage(command.Args);
                break;
            case "name":
                conn.RemoteReducers.SetName(command.Args);
                break;
        }
    }
}

Main();
