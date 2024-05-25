---
title: Python Client SDK Quick Start
---

In this guide, we'll show you how to get up and running with a simple SpacetimDB app with a client written in Python.

We'll implement a command-line client for the module created in our [Rust Module Quickstart](/docs/modules/rust/quickstart) or [C# Module Quickstart](/docs/modules/c-charp/quickstart) guides. Make sure you follow one of these guides before you start on this one.

## Install the SpacetimeDB SDK Python Package

1. Run pip install

```bash
pip install spacetimedb_sdk
```

## Project structure

Enter the directory `quickstart-chat` you created in the Rust or C# Module Quickstart guides and create a `client` folder:

```bash
cd quickstart-chat
mkdir client
```

## Create the Python main file

Create a file called `main.py` in the `client` and open it in your favorite editor. We prefer [VS Code](https://code.visualstudio.com/).

## Add imports

We need to add several imports for this quickstart:

-   [`asyncio`](https://docs.python.org/3/library/asyncio.html) is required to run the async code in the SDK.
-   [`multiprocessing.Queue`](https://docs.python.org/3/library/multiprocessing.html) allows us to pass our input to the async code, which we will run in a separate thread.
-   [`threading`](https://docs.python.org/3/library/threading.html) allows us to spawn our async code in a separate thread so the main thread can run the input loop.

-   `spacetimedb_sdk.spacetimedb_async_client.SpacetimeDBAsyncClient` is the async wrapper around the SpacetimeDB client which we use to interact with our SpacetimeDB module.
-   `spacetimedb_sdk.local_config` is an optional helper module to load the auth token from local storage.

```python
import asyncio
from multiprocessing import Queue
import threading

from spacetimedb_sdk.spacetimedb_async_client import SpacetimeDBAsyncClient
import spacetimedb_sdk.local_config as local_config
```

## Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `client` directory, run:

```bash
mkdir -p module_bindings
spacetime generate --lang python --out-dir module_bindings --project-path ../server
```

Take a look inside `client/module_bindings`. The CLI should have generated five files:

```
module_bindings
+-- message.py
+-- send_message_reducer.py
+-- set_name_reducer.py
+-- user.py
```

Now we import these types by adding the following lines to `main.py`:

```python
import module_bindings
from module_bindings.user import User
from module_bindings.message import Message
import module_bindings.send_message_reducer as send_message_reducer
import module_bindings.set_name_reducer as set_name_reducer
```

## Global variables

Next we will add our global `input_queue` and `local_identity` variables which we will explain later when they are used.

```python
input_queue = Queue()
local_identity = None
```

## Define main function

We'll work outside-in, first defining our `main` function at a high level, then implementing each behavior it needs. We need `main` to do four things:

1. Init the optional local config module. The first parameter is the directory name to be created in the user home directory.
1. Create our async SpacetimeDB client.
1. Register our callbacks.
1. Start the async client in a thread.
1. Run a loop to read user input and send it to a repeating event in the async client.
1. When the user exits, stop the async client and exit the program.

```python
if __name__ == "__main__":
    local_config.init(".spacetimedb-python-quickstart")

    spacetime_client = SpacetimeDBAsyncClient(module_bindings)

    register_callbacks(spacetime_client)

    thread = threading.Thread(target=run_client, args=(spacetime_client,))
    thread.start()

    input_loop()

    spacetime_client.force_close()
    thread.join()
```

## Register callbacks

We need to handle several sorts of events:

1. OnSubscriptionApplied is a special callback that is executed when the local client cache is populated. We will talk more about this later.
2. When a new user joins or a user is updated, we'll print an appropriate message.
3. When we receive a new message, we'll print it.
4. If the server rejects our attempt to set our name, we'll print an error.
5. If the server rejects a message we send, we'll print an error.
6. We use the `schedule_event` function to register a callback to be executed after 100ms. This callback will check the input queue for any user input and execute the appropriate command.

Because python requires functions to be defined before they're used, the following code must be added to `main.py` before main block:

```python
def register_callbacks(spacetime_client):
    spacetime_client.client.register_on_subscription_applied(on_subscription_applied)

    User.register_row_update(on_user_row_update)
    Message.register_row_update(on_message_row_update)

    set_name_reducer.register_on_set_name(on_set_name_reducer)
    send_message_reducer.register_on_send_message(on_send_message_reducer)

    spacetime_client.schedule_event(0.1, check_commands)
```

### Handling User row updates

For each table, we can register a row update callback to be run whenever a subscribed row is inserted, updated or deleted. We register these callbacks using the `register_row_update` methods that are generated automatically for each table by `spacetime generate`.

These callbacks can fire in two contexts:

-   After a reducer runs, when the client's cache is updated about changes to subscribed rows.
-   After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User::row_update` callbacks may be invoked with users who are offline. We'll only notify about online users.

We are also going to check for updates to the user row. This can happen for three reasons:

1. They've set their name using the `set_name` reducer.
2. They're an existing user re-connecting, so their `online` has been set to `true`.
3. They've disconnected, so their `online` has been set to `false`.

We'll print an appropriate message in each of these cases.

`row_update` callbacks take four arguments: the row operation ("insert", "update", or "delete"), the old row if it existed, the new or updated row, and a `ReducerEvent`. This will `None` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is an class that contains information about the reducer that triggered this row update event.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `user_name_or_identity` handle this.

Add these functions before the `register_callbacks` function:

```python
def user_name_or_identity(user):
    if user.name:
        return user.name
    else:
        return (str(user.identity))[:8]

def on_user_row_update(row_op, user_old, user, reducer_event):
    if row_op == "insert":
        if user.online:
            print(f"User {user_name_or_identity(user)} connected.")
    elif row_op == "update":
        if user_old.online and not user.online:
            print(f"User {user_name_or_identity(user)} disconnected.")
        elif not user_old.online and user.online:
            print(f"User {user_name_or_identity(user)} connected.")

        if user_old.name != user.name:
            print(
                f"User {user_name_or_identity(user_old)} renamed to {user_name_or_identity(user)}."
            )
```

### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `on_message_row_update` callback will check if its `reducer_event` argument is not `None`, and only print in that case.

To find the `User` based on the message's `sender` identity, we'll use `User::filter_by_identity`, which behaves like the same function on the server. The key difference is that, unlike on the module side, the client's `filter_by_identity` accepts a `bytes`, rather than an `&Identity`. The `sender` identity stored in the message is also a `bytes`, not an `Identity`, so we can just pass it to the filter method.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

Add these functions before the `register_callbacks` function:

```python
def on_message_row_update(row_op, message_old, message, reducer_event):
    if reducer_event is not None and row_op == "insert":
        print_message(message)

def print_message(message):
    user = User.filter_by_identity(message.sender)
    user_name = "unknown"
    if user is not None:
        user_name = user_name_or_identity(user)

    print(f"{user_name}: {message.text}")
```

### Warn if our name was rejected

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `register_on_<reducer>` method, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes four fixed arguments:

1. The `Identity` of the client who requested the reducer invocation.
2. The `Address` of the client who requested the reducer invocation, or `None` for scheduled reducers.
3. The `Status` of the reducer run, one of `committed`, `failed` or `outofenergy`.
4. The `Message` returned by the reducer in error cases, or `None` if the reducer succeeded.

It also takes a variable number of arguments which match the calling arguments of the reducer.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `failed` or `outofenergy` implies that the caller identity is our own identity.

We already handle successful `set_name` invocations using our `User::on_update` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `on_set_name_reducer` as a callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.

We'll test both that our identity matches the sender and that the status is `failed`, even though the latter implies the former, for demonstration purposes.

Add this function before the `register_callbacks` function:

```python
def on_set_name_reducer(sender_id, sender_address, status, message, name):
    if sender_id == local_identity:
        if status == "failed":
            print(f"Failed to set name: {message}")
```

### Warn if our message was rejected

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.

Add this function before the `register_callbacks` function:

```python
def on_send_message_reducer(sender_id, sender_address, status, message, msg):
    if sender_id == local_identity:
        if status == "failed":
            print(f"Failed to send message: {message}")
```

### OnSubscriptionApplied callback

This callback fires after the client cache is updated as a result in a change to the client subscription. This happens after connect and if after calling `subscribe` to modify the subscription.

In this case, we want to print all the existing messages when the subscription is applied. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `Message.iter()` is generated for all table types, and returns an iterator over all the messages in the client's cache.

Add these functions before the `register_callbacks` function:

```python
def print_messages_in_order():
    all_messages = sorted(Message.iter(), key=lambda x: x.sent)
    for entry in all_messages:
        print(f"{user_name_or_identity(User.filter_by_identity(entry.sender))}: {entry.text}")

def on_subscription_applied():
    print(f"\nSYSTEM: Connected.")
    print_messages_in_order()
```

### Check commands repeating event

We'll use a repeating event to check the user input queue every 100ms. If there's a command in the queue, we'll execute it. If not, we'll just keep waiting. Notice that at the end of the function we call `schedule_event` again to so the event will repeat.

If the command is to send a message, we'll call the `send_message` reducer. If the command is to set our name, we'll call the `set_name` reducer.

Add these functions before the `register_callbacks` function:

```python
def check_commands():
    global input_queue

    if not input_queue.empty():
        choice = input_queue.get()
        if choice[0] == "name":
            set_name_reducer.set_name(choice[1])
        else:
            send_message_reducer.send_message(choice[1])

    spacetime_client.schedule_event(0.1, check_commands)
```

### OnConnect callback

This callback fires after the client connects to the server. We'll use it to save our credentials to a file so that we can re-authenticate as the same user next time we connect.

The `on_connect` callback takes three arguments:

1. The `Auth Token` is the equivalent of your private key. This is the only way to authenticate with the SpacetimeDB module as this user.
2. The `Identity` is the equivalent of your public key. This is used to uniquely identify this user and will be sent to other clients. We store this in a global variable so we can use it to identify that a given message or transaction was sent by us.
3. The `Address` is an opaque identifier modules can use to distinguish multiple concurrent connections by the same `Identity`. We don't need to know our `Address`, so we'll ignore that argument.

To store our auth token, we use the optional component `local_config`, which provides a simple interface for storing and retrieving a single `Identity` from a file. We'll use the `local_config::set_string` method to store the auth token. Other projects might want to associate this token with some other identifier such as an email address or Steam ID.

The `on_connect` callback is passed to the client connect function so it just needs to be defined before the `run_client` described next.

```python
def on_connect(auth_token, identity):
    global local_identity
    local_identity = identity

    local_config.set_string("auth_token", auth_token)
```

## Async client thread

We are going to write a function that starts the async client, which will be executed on a separate thread.

```python
def run_client(spacetime_client):
    asyncio.run(
        spacetime_client.run(
            local_config.get_string("auth_token"),
            "localhost:3000",
            "chat",
            False,
            on_connect,
            ["SELECT * FROM User", "SELECT * FROM Message"],
        )
    )
```

## Input loop

Finally, we need a function to be executed on the main loop which listens for user input and adds it to the queue.

```python
def input_loop():
    global input_queue

    while True:
        user_input = input()
        if len(user_input) == 0:
            return
        elif user_input.startswith("/name "):
            input_queue.put(("name", user_input[6:]))
        else:
            input_queue.put(("message", user_input))
```

## Run the client

Make sure your module from the Rust or C# module quickstart is published. If you used a different module name than `chat`, you will need to update the `connect` call in the `run_client` function.

Run the client:

```bash
python main.py
```

If you want to connect another client, you can use the --client command line option, which is built into the local_config module. This will create different settings file for the new client's auth token.

```bash
python main.py --client 2
```

## Next steps

Congratulations! You've built a simple chat app with a Python client. You can now use this as a starting point for your own SpacetimeDB apps.

For a more complex example of the Spacetime Python SDK, check out our [AI Agent](https://github.com/clockworklabs/spacetime-mud/tree/main/ai-agent-python-client) for the [Spacetime Multi-User Dungeon](https://github.com/clockworklabs/spacetime-mud). The AI Agent uses the OpenAI API to create dynamic content on command.
