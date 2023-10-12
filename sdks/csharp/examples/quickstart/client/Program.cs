using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;

// our local client SpacetimeDB identity
Identity? local_identity = null;
// declare a thread safe queue to store commands in format (command, args)
ConcurrentQueue<(string,string)> input_queue = new ConcurrentQueue<(string, string)>();
// declare a threadsafe cancel token to cancel the process loop
CancellationTokenSource cancel_token = new CancellationTokenSource();

void Main()
{
    AuthToken.Init(".spacetime_csharp_quickstart");

    // create the client, pass in a logger to see debug messages
    SpacetimeDBClient.CreateInstance(new ConsoleLogger());

    RegisterCallbacks();

    // spawn a thread to call process updates and process commands
    var thread = new Thread(ProcessThread);
    thread.Start();

    InputLoop();

    // this signals the ProcessThread to stop
    cancel_token.Cancel();
    thread.Join();
}

void RegisterCallbacks()
{
    SpacetimeDBClient.instance.onConnect += OnConnect;
    SpacetimeDBClient.instance.onIdentityReceived += OnIdentityReceived;
    SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;

    User.OnInsert += User_OnInsert;
    User.OnUpdate += User_OnUpdate;

    Message.OnInsert += Message_OnInsert;

    Reducer.OnSetNameEvent += Reducer_OnSetNameEvent;
    Reducer.OnSendMessageEvent += Reducer_OnSendMessageEvent;
}

string UserNameOrIdentity(User user) => user.Name ?? user.Identity.ToString()!.Substring(0, 8);

void User_OnInsert(User insertedValue, ReducerEvent? dbEvent)
{
    if(insertedValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(insertedValue)} is online");
    }
}

void User_OnUpdate(User oldValue, User newValue, ReducerEvent dbEvent)
{
    if(oldValue.Name != newValue.Name)
    {
        Console.WriteLine($"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}");
    }
    if(oldValue.Online != newValue.Online)
    {
        if(newValue.Online)
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
    var sender = User.FilterByIdentity(message.Sender);
    var senderName = "unknown";
    if(sender != null)
    {
        senderName = UserNameOrIdentity(sender);
    }

    Console.WriteLine($"{senderName}: {message.Text}");
}

void Message_OnInsert(Message insertedValue, ReducerEvent? dbEvent)
{
    if(dbEvent != null)
    {
        PrintMessage(insertedValue);
    }
}

void Reducer_OnSetNameEvent(ReducerEvent reducerEvent, string name)
{
    if(reducerEvent.Identity == local_identity && reducerEvent.Status == ClientApi.Event.Types.Status.Failed)
    {
        Console.Write($"Failed to change name to {name}");
    }
}

void Reducer_OnSendMessageEvent(ReducerEvent reducerEvent, string text)
{
    if (reducerEvent.Identity == local_identity && reducerEvent.Status == ClientApi.Event.Types.Status.Failed)
    {
        Console.Write($"Failed to send message {text}");
    }
}

void OnConnect()
{
    SpacetimeDBClient.instance.Subscribe(new List<string> { "SELECT * FROM User", "SELECT * FROM Message" });
}

void OnIdentityReceived(string authToken, Identity identity, Address _address)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);
}

void PrintMessagesInOrder()
{
    foreach (Message message in Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(message);
    }
}

void OnSubscriptionApplied()
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder();
}

const string HOST = "http://localhost:3000";
const string DBNAME = "module";

void ProcessThread()
{
    SpacetimeDBClient.instance.Connect(AuthToken.Token, HOST, DBNAME);

    // loop until cancellation token
    while (!cancel_token.IsCancellationRequested)
    {
        SpacetimeDBClient.instance.Update();

        ProcessCommands();

        Thread.Sleep(100);
    }

    SpacetimeDBClient.instance.Close();
}

void InputLoop()
{
    while (true)
    {
        var input = Console.ReadLine();
        if(input == null)
        {
            break;
        }

        if(input.StartsWith("/name "))
        {
            input_queue.Enqueue(("name", input.Substring(6)));
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
        switch (command.Item1)
        {
            case "message":
                Reducer.SendMessage(command.Item2);
                break;
            case "name":
                Reducer.SetName(command.Item2);
                break;
        }
    }
}

Main();
