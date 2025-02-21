// Multiplicity Test Client
// This test adds several dogs to the Multiplicity Test Server,
// stores a local model of those dogs,
// creates several connections in different combinations,
// and compares the server-version to the local model.

using client;
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;

// Configure Output settings
bool show_on_insert_events_output = true;
bool show_on_update_events_output = true;
bool show_on_delete_events_output = true;
bool show_reducer_events_output = true;

// Private variables
Model model = new Model();
// declare a thread safe queue to store commands
var command_queue = new ConcurrentQueue<(string Command, string name, string color, uint age)>();
bool ready_for_command = false;

SubscriptionHandle<SubscriptionEventContext, ErrorContext>? primaryTestSubscriptionHandle = null;
SubscriptionHandle<SubscriptionEventContext, ErrorContext>? secondaryTestSubscriptionHandle = null;

void Main()
{
    AuthToken.Init(".spacetime_csharp_multiplicity");

    // Builds and connects to the database
    DbConnection? conn = null;
    conn = ConnectToDB();
    // Registers callbacks for reducers
    RegisterCallbacks(conn);
    // Declare a threadsafe cancel token to cancel the process loop
    var cancellationTokenSource = new CancellationTokenSource();
    // Spawn a thread to call process updates and process commands
    var thread = new Thread(() => ProcessThread(conn, cancellationTokenSource.Token));
    thread.Start();
    // Tests start here
    Test1();
    Test2();
    // Handles CLI input
    InputLoop();
    // This signals the ProcessThread to stop
    cancellationTokenSource.Cancel();
    thread.Join();
}

const string HOST = "http://localhost:3000";
const string DBNAME = "multiplicity";

DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DBNAME)
        .WithToken(AuthToken.Token)
        .OnConnect(OnConnected)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnect)
        .Build();
    return conn;
}

void RegisterCallbacks(DbConnection conn)
{
    conn.Db.Dog.OnInsert += Dog_OnInsert;
    conn.Db.Dog.OnUpdate += Dog_OnUpdate;
    conn.Db.Dog.OnDelete += Dog_OnDelete;
    
    conn.Db.Cat.OnInsert += Cat_OnInsert;
    conn.Db.Cat.OnUpdate += Cat_OnUpdate;
    conn.Db.Cat.OnDelete += Cat_OnDelete;
    
    conn.Reducers.OnAddDog += Reducer_OnAddDogEvent;
    conn.Reducers.OnUpdateDog += Reducer_OnUpdateDogEvent;
    conn.Reducers.OnRemoveDog += Reducer_OnRemoveDogEvent;

    conn.Reducers.OnAddCat += Reducer_OnAddCatEvent;
    conn.Reducers.OnUpdateCat += Reducer_OnUpdateCatEvent;
    conn.Reducers.OnRemoveCat += Reducer_OnRemoveCatEvent;
}

# region Event Handlers
void Dog_OnInsert(EventContext ctx, Dog insertedValue)
{
    if (show_on_insert_events_output) Console.WriteLine($"EventContext: Dog (Name:{insertedValue.Name}, Color:{insertedValue.Color}, Age:{insertedValue.Age}) inserted.");
}

void Dog_OnUpdate(EventContext ctx, Dog oldValue, Dog newValue)
{
    if (show_on_update_events_output) Console.WriteLine($"EventContext: Dog (Name:{oldValue.Name}, Color:{oldValue.Color}, Age:{oldValue.Age}) updated to (Name:{newValue.Name}, Color:{newValue.Color}, Age:{newValue.Age}).");
}

void Dog_OnDelete(EventContext ctx, Dog deletedValue)
{
    if (show_on_delete_events_output) Console.WriteLine($"EventContext: Dog (Name:{deletedValue.Name}, Color:{deletedValue.Color}, Age:{deletedValue.Age}) deleted.");
}

void Cat_OnInsert(EventContext ctx, Cat insertedValue)
{
    if (show_on_insert_events_output) Console.WriteLine($"EventContext: Cat (Name:{insertedValue.Name}, Color:{insertedValue.Color}, Age:{insertedValue.Age}) inserted.");
}

void Cat_OnUpdate(EventContext ctx, Cat oldValue, Cat newValue)
{
    if (show_on_update_events_output) Console.WriteLine($"EventContext: Cat (Name:{oldValue.Name}, Color:{oldValue.Color}, Age:{oldValue.Age}) updated to (Name:{newValue.Name}, Color:{newValue.Color}, Age:{newValue.Age}).");
}

void Cat_OnDelete(EventContext ctx, Cat deletedValue)
{
    if (show_on_delete_events_output) Console.WriteLine($"EventContext: Cat (Name:{deletedValue.Name}, Color:{deletedValue.Color}, Age:{deletedValue.Age}) deleted.");
}
# endregion

# region Reducer Events

void Reducer_OnAddDogEvent(ReducerEventContext ctx, string name, string color, uint age)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Add Event Dog (Name:{name}, Color:{color}, Age:{age}) called. Adding dog to local model.");
    model.AddDog(new Dog(name, color, age));
    ready_for_command = true;
}

void Reducer_OnUpdateDogEvent(ReducerEventContext ctx, string name, string color, uint age)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Update Event Dog (Name:{name}, Color:{color}, Age:{age}) called. Updating dog in local model.");
    model.UpdateDog(new Dog(name, color, age));
    ready_for_command = true;
}

void Reducer_OnRemoveDogEvent(ReducerEventContext ctx, string name)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Remove Event Dog (Name:{name}) called. Removing dog from local model.");
    if (model.ContainsDog(name)) model.RemoveDog(name);
    ready_for_command = true;
}

void Reducer_OnAddCatEvent(ReducerEventContext ctx, string name, string color, uint age)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Add Event Cat (Name:{name}, Color:{color}, Age:{age}) called. Adding cat to local model.");
    model.AddCat(new Cat(name, color, age));
    ready_for_command = true;
}

void Reducer_OnUpdateCatEvent(ReducerEventContext ctx, string name, string color, uint age)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Update Event Cat (Name:{name}, Color:{color}, Age:{age}) called. Updating cat in local model.");
    model.UpdateCat(new Cat(name, color, age));
    ready_for_command = true;
}

void Reducer_OnRemoveCatEvent(ReducerEventContext ctx, string name)
{
    if (show_reducer_events_output) Console.WriteLine($"ReducerEventContext: Remove Event Cat (Name:{name}) called. Removing cat from local model.");
    if (model.ContainsCat(name)) model.RemoveCat(name);
    ready_for_command = true;
}

# endregion

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    AuthToken.SaveToken(authToken);
    
    ready_for_command = true;
}

void OnConnectError(Exception e)
{
    Console.Write($"Error while connecting: {e}");
}

void OnDisconnect(DbConnection conn, Exception? e)
{
    if (e != null)
    {
        Console.Write($"Disconnected abnormally: {e}");
    } else {
        Console.Write($"Disconnected normally.");
    }
}

void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Subscription Applied");
    ready_for_command = true;
}

void OutputSubscribedServerDogs(DbConnection conn)
{
    Console.WriteLine("Subscribed Server dogs:");
    foreach (Dog dog in conn.Db.Dog.Iter())
    {
        Console.WriteLine($"  Dog (Name:{dog.Name}, Color:{dog.Color}, Age:{dog.Age}).");
    }
}

void OutputSubscribedServerCats(DbConnection conn)
{
    Console.WriteLine("Subscribed Server cats:");
    foreach (Cat cat in conn.Db.Cat.Iter())
    {
        Console.WriteLine($"  Cat (Name:{cat.Name}, Color:{cat.Color}, Age:{cat.Age}).");
    }
}

void CompareEventDogsToModel(DbConnection conn, HashSet<Dog> modelHashSet)
{
    bool allMatched = true;
    Console.WriteLine("Comparing Server dogs to Model:");
    foreach (Dog dog in conn.Db.Dog.Iter())
    {
        if (model.ContainsDog(dog.Name, dog.Color, dog.Age, modelHashSet) == false)
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"Dog (Name:{dog.Name}, Color:{dog.Color}, Age:{dog.Age}) was missing from local model.");
            Console.ForegroundColor = ConsoleColor.White;
            allMatched = false;
        }
    }

    foreach (Dog expectedDog in modelHashSet)
    {
        bool found = false;
        foreach (Dog dog in conn.Db.Dog.Iter())
        {
            if (expectedDog.Name == dog.Name && expectedDog.Color == dog.Color && expectedDog.Age == dog.Age)
            {
                found = true;
                break;
            }
        }

        if (!found)
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"Dog (Name:{expectedDog.Name}, Color:{expectedDog.Color}, Age:{expectedDog.Age}) was missing from server model.");
            Console.ForegroundColor = ConsoleColor.White;
            allMatched = false;
        }
    }

    if (allMatched)
    {
        Console.ForegroundColor = ConsoleColor.Green;
        Console.WriteLine($"All dogs on server and model are equal.");
        Console.ForegroundColor = ConsoleColor.White;
    }
}

void CompareEventCatsToModel(DbConnection conn, HashSet<Cat> modelHashSet)
{
    bool allMatched = true;
    Console.WriteLine("Comparing Server cats to Model:");
    foreach (Cat cat in conn.Db.Cat.Iter())
    {
        if (model.ContainsCat(cat.Name, cat.Color, cat.Age, modelHashSet) == false)
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"Cat (Name:{cat.Name}, Color:{cat.Color}, Age:{cat.Age}) was missing from local model.");
            Console.ForegroundColor = ConsoleColor.White;
            allMatched = false;
        }
    }

    foreach (Cat expectedCat in modelHashSet)
    {
        bool found = false;
        foreach (Cat cat in conn.Db.Cat.Iter())
        {
            if (expectedCat.Name == cat.Name && expectedCat.Color == cat.Color && expectedCat.Age == cat.Age)
            {
                found = true;
                break;
            }
        }

        if (!found)
        {
            Console.ForegroundColor = ConsoleColor.Red;
            Console.WriteLine($"Cat (Name:{expectedCat.Name}, Color:{expectedCat.Color}, Age:{expectedCat.Age}) was missing from server model.");
            Console.ForegroundColor = ConsoleColor.White;
            allMatched = false;
        }
    }

    if (allMatched)
    {
        Console.ForegroundColor = ConsoleColor.Green;
        Console.WriteLine($"All cats on server and model are equal.");
        Console.ForegroundColor = ConsoleColor.White;
    }
}

void ProcessThread(DbConnection conn, CancellationToken ct)
{
    try
    {
        // loop until cancellation token
        while (!ct.IsCancellationRequested)
        {
            conn.FrameTick();
            
            if (ready_for_command) ProcessCommands(conn);
            
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
    }
}

void AddStartingDogs()
{
    command_queue.Enqueue(("log", "","", 0));
    command_queue.Enqueue(("log", "Adding Default Dogs","", 0));
    command_queue.Enqueue(("add_dog", "Alpha","Black", 3));
    command_queue.Enqueue(("add_dog", "Beau","Brown", 4));
    command_queue.Enqueue(("add_dog", "Chance","White", 4));
    command_queue.Enqueue(("add_dog", "Dante","Grey", 3));
    command_queue.Enqueue(("add_dog", "Einstein","Brown", 3));
    command_queue.Enqueue(("add_dog", "Foo-Foo","Brown", 2));
    command_queue.Enqueue(("add_dog", "Georgette","White", 3));
    command_queue.Enqueue(("add_dog", "Hansel","Black", 2));
    command_queue.Enqueue(("add_dog", "Isaac","Black", 2));
    command_queue.Enqueue(("add_dog", "Shadow","Golden", 6));
}

void RemoveStartingDogs()
{
    command_queue.Enqueue(("log", "","", 0));
    command_queue.Enqueue(("log", "Removing Default Dogs","", 0));
    command_queue.Enqueue(("remove_dog", "Alpha","Black", 3));
    command_queue.Enqueue(("remove_dog", "Beau","Brown", 4));
    command_queue.Enqueue(("remove_dog", "Chance","White", 4));
    command_queue.Enqueue(("remove_dog", "Dante","Grey", 3));
    command_queue.Enqueue(("remove_dog", "Einstein","Brown", 3));
    command_queue.Enqueue(("remove_dog", "Foo-Foo","Brown", 2));
    command_queue.Enqueue(("remove_dog", "Georgette","White", 3));
    command_queue.Enqueue(("remove_dog", "Hansel","Black", 2));
    command_queue.Enqueue(("remove_dog", "Isaac","Black", 2));
    command_queue.Enqueue(("remove_dog", "Shadow","Golden", 6));
}

void Test1()
{
    AddStartingDogs();
    command_queue.Enqueue(("log", "","", 0));
    command_queue.Enqueue(("log", "=== Starting test 1: Adding/Removing records for a single Connection Handle with multiple Subscriptions ===","", 0));
    command_queue.Enqueue(("log", "--- Using a string array to subscribe to Only Brown Dogs and Dogs older than 3 ---","", 0));
    command_queue.Enqueue(("subscribe_to_test_1", "","", 0));
    command_queue.Enqueue(("set_client_dogs_test1", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Updating Dog \"Georgette\" to age 4, which should cause Georgette to be included in the model. ---","", 0));
    command_queue.Enqueue(("update_dog", "Georgette","White", 4));
    command_queue.Enqueue(("set_client_dogs_test1", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Updating Dog \"Foo-Foo\" to color \"Grey\", which should remove Foo-Foo from the model. ---","", 0));
    command_queue.Enqueue(("update_dog", "Foo-Foo","Grey", 2));
    command_queue.Enqueue(("set_client_dogs_test1", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Test 1 complete, unsubscribing ---","", 0));
    command_queue.Enqueue(("unsubscribe_to_test_1", "","", 0));
    RemoveStartingDogs();
}

void Test2()
{
    AddStartingDogs();
    command_queue.Enqueue(("log", "","", 0));
    command_queue.Enqueue(("log", "=== Starting test 2: Adding/Removing multiple overlapping Connection Handles ===","", 0));
    command_queue.Enqueue(("log", "--- Using one connection handle to subscribe to Only Brown Dogs and another connection handle to subscribe to Dogs older than 3 ---","", 0));
    command_queue.Enqueue(("subscribe_to_test_2", "","", 0));
    command_queue.Enqueue(("set_client_dogs_test2", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Unsubscribing handle of Only Brown Dogs ---","", 0));
    command_queue.Enqueue(("unsubscribe_to_primary_test_2", "","", 0));
    
    command_queue.Enqueue(("log", "--- Updating Dog \"Georgette\" to age 4, which should cause Georgette to be included in the model. ---","", 0));
    command_queue.Enqueue(("update_dog", "Georgette","White", 4));
    command_queue.Enqueue(("set_client_dogs_test2", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Updating Dog \"Foo-Foo\" to color \"Grey\", which should remove Foo-Foo from the model. ---","", 0));
    command_queue.Enqueue(("update_dog", "Foo-Foo","Grey", 2));
    command_queue.Enqueue(("set_client_dogs_test2", "","", 0));
    command_queue.Enqueue(("compare_client_dogs_to_server", "","", 0));
    command_queue.Enqueue(("print_server_dogs", "","", 0));
    command_queue.Enqueue(("print_client_dogs", "","", 0));
    
    command_queue.Enqueue(("log", "--- Test complete, unsubscribing handle of Dogs older than 3 ---","", 0));
    command_queue.Enqueue(("unsubscribe_to_secondary_test_2", "","", 0));
    RemoveStartingDogs();
}

void ProcessCommands(DbConnection conn)
{
    // process command queue
    while (ready_for_command == true && command_queue.TryDequeue(out var command))
    {
        switch (command.Command)
        {
            case "log":
                Console.WriteLine(command.name);
                break;
            case "add_dog":
                ready_for_command = false;
                conn.Reducers.AddDog(command.name, command.color, command.age);
                break;
            case "add_cat":
                ready_for_command = false;
                conn.Reducers.AddCat(command.name, command.color, command.age);
                break;
            case "update_dog":
                ready_for_command = false;
                conn.Reducers.UpdateDog(command.name, command.color, command.age);
                break;
            case "update_cat":
                ready_for_command = false;
                conn.Reducers.UpdateCat(command.name, command.color, command.age);
                break;
            case "remove_dog":
                ready_for_command = false;
                conn.Reducers.RemoveDog(command.name);
                break;
            case "remove_cat":
                ready_for_command = false;
                conn.Reducers.RemoveCat(command.name);
                break;
            case "subscribe_to_test_1":
                ready_for_command = false;
                string[] subscriptionArray1 = new string[] { "SELECT * FROM dog WHERE dog.age > 3", "SELECT * FROM dog WHERE dog.color = 'Brown'" };
                primaryTestSubscriptionHandle = conn.SubscriptionBuilder()
                    .OnApplied(OnSubscriptionApplied)
                    .Subscribe(subscriptionArray1);
                break;
            case "set_client_dogs_test1":
                model.ExpectedClientDogs = new HashSet<Dog>(model.ExpectedServerDogs.Where(dog => dog.Age > 3 || dog.Color == "Brown"));
                break;
            case "unsubscribe_to_test_1":
                primaryTestSubscriptionHandle?.Unsubscribe();
                break;
            case "subscribe_to_test_2":
                ready_for_command = false;
                string[] primarySubscriptionArray = new string[] { "SELECT * FROM dog WHERE dog.age > 3" };
                string[] secondaySubscriptionArray = new string[] { "SELECT * FROM dog WHERE dog.color = 'Brown'" };
                primaryTestSubscriptionHandle = conn.SubscriptionBuilder()
                    .OnApplied(OnSubscriptionApplied)
                    .Subscribe(primarySubscriptionArray);
                secondaryTestSubscriptionHandle = conn.SubscriptionBuilder()
                    .OnApplied(OnSubscriptionApplied)
                    .Subscribe(secondaySubscriptionArray);
                break;
            case "set_client_dogs_test2":
                model.ExpectedClientDogs = new HashSet<Dog>(model.ExpectedServerDogs.Where(dog => dog.Age > 3 || dog.Color == "Brown"));
                break;
            case "unsubscribe_to_primary_test_2":
                primaryTestSubscriptionHandle?.Unsubscribe();
                break;
            case "unsubscribe_to_secondary_test_2":
                secondaryTestSubscriptionHandle?.Unsubscribe();
                break;
            case "compare_client_dogs_to_server":
                CompareEventDogsToModel(conn, model.ExpectedClientDogs);
                break;
            case "compare_client_cats_to_server":
                CompareEventCatsToModel(conn, model.ExpectedClientCats);
                break;
            case "set_client_dogs_to_server":
                model.ExpectedClientDogs = model.ExpectedServerDogs;
                break;
            case "set_client_cats_to_server":
                model.ExpectedClientCats = model.ExpectedServerCats;
                break;
            case "set_client_cats_test1":
                model.ExpectedClientCats = new HashSet<Cat>(model.ExpectedServerCats.Where(cat => cat.Age > 2));
                break;
            case "print_server_dogs":
                OutputSubscribedServerDogs(conn);
                break;
            case "print_server_cats":
                OutputSubscribedServerCats(conn);
                break;
            case "print_client_dogs":
                model.OutputExpectedDogs(model.ExpectedClientDogs);
                break;
            case "print_client_cats":
                model.OutputExpectedCats(model.ExpectedClientCats);
                break;
        }
    }
}

Main();