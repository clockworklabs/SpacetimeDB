mod module_bindings;
use module_bindings::*;

use spacetimedb_sdk::{
    disconnect,
    identity::{identity, load_credentials, once_on_connect, save_credentials, Credentials, Identity},
    on_disconnect, on_subscription_applied,
    reducer::Status,
    subscribe,
    table::{TableType, TableWithPrimaryKey},
    Address,
};

use cursive::{
    traits::*,
    views::{Dialog, EditView, LinearLayout, PaddedView, ScrollView, TextView},
    Cursive, CursiveRunnable, CursiveRunner,
};
use futures_channel::mpsc;

// # Our main function

fn main() {
    // We'll pre-process database events in callbacks,
    // then send `UiMessage` events over a channel
    // to the `user_input_loop`.
    let (ui_send, ui_recv) = mpsc::unbounded();

    // Each of our callbacks will need a handle on the `UiMessage` channel.
    register_callbacks(ui_send);

    // Connecting and subscribing are unchanged relative to the quickstart client.
    connect_to_db();
    subscribe_to_tables();

    // We'll build a Cursive TUI,
    let ui = make_ui();
    // then run it manually in our own loop,
    // rather than using the Cursive event loop,
    // so that we can push events into it.
    user_input_loop(ui, ui_recv);
    disconnect();
}

enum UiMessage {
    /// A remote user has connected to the server,
    /// so add them to the online users view.
    UserConnected {
        /// Used as a Cursive element name,
        /// to identify views which refer to this user,
        /// i.e. their online status and their messages.
        identity: Identity,
        /// Displayed in the online status view.
        name: String,
    },
    /// A remote user has disconnected from the server,
    /// so remove them from the online users view.
    UserDisconnected {
        /// Used to locate the user's entry in the online status view.
        identity: Identity,
    },
    /// We have successfully set our own name,
    /// so update our past messages and the name in the input bar.
    SetOwnName { name: String },
    /// A remote user has set their name,
    /// so update their past messages and their online status
    /// to use the new name.
    SetName {
        /// Used to locate the user's entry in the online status view
        /// and their past messages.
        identity: Identity,
        /// Will be placed in the user's online status view
        /// and past messages.
        new_name: String,
    },
    /// Someone sent a new message,
    /// so add it to the messages view.
    Message {
        /// Will be displayed as the sender.
        sender_name: String,
        /// Used as a Cursive element name to identify the user,
        /// so that we can update the sender name if they change their name.
        sender_identity: Identity,
        /// The text of the message.
        text: String,
    },
    /// We sent a message that was rejected by the server,
    /// so display a pop-up dialog.
    MessageRejected {
        /// The text of the rejected message, to be included in the dialog.
        rejected_message: String,
        /// The server error message, to be included in the dialog.
        reason: String,
    },
    /// We tried to set our name but were rejected by the server,
    /// so display a pop-up dialog and reset our name in the input bar.
    NameRejected {
        /// The current name, to be placed in the input bar.
        current_name: String,
        /// The rejected name, to be included in the dialog.
        rejected_name: String,
        /// The server error message, to be included in the dialog.
        reason: String,
    },
    /// Our connection has ended.
    ConnectionClosed,
}

type UiSend = mpsc::UnboundedSender<UiMessage>;
type UiRecv = mpsc::UnboundedReceiver<UiMessage>;

// # Register callbacks

/// Register all the callbacks our app will use to respond to database events.
fn register_callbacks(send: UiSend) {
    // When we receive our `Credentials`, save them to a file.
    once_on_connect(on_connected);

    // When a new user joins, print a notification.
    User::on_insert(on_user_inserted(send.clone()));

    // When a user's status changes, print a notification.
    User::on_update(on_user_updated(send.clone()));

    // When a new message is received, print it.
    Message::on_insert(on_message_inserted(send.clone()));

    // When we receive the message backlog, print it in timestamp order.
    on_subscription_applied(on_sub_applied(send.clone()));

    // When we fail to set our name, print a warning.
    on_set_name(on_name_set(send.clone()));

    // When we fail to send a message, print a warning.
    on_send_message(on_message_sent(send.clone()));

    // When our connection closes, show a dialog, then exit the process.
    on_disconnect(on_disconnected(send.clone()));
}

// ## Save credentials to a file

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(creds: &Credentials, _address: Address) {
    if let Err(e) = save_credentials(CREDS_DIR, creds) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}

const CREDS_DIR: &str = ".spacetime_chat";

// ## Notify about new users

/// Our `User::on_insert` callback: if the user is online, print a notification.
fn on_user_inserted(send: UiSend) -> impl FnMut(&User, Option<&ReducerEvent>) + Send + 'static {
    move |user, _| {
        if user.identity == identity().unwrap() {
            send.unbounded_send(UiMessage::SetOwnName {
                name: user_name_or_identity(user),
            })
            .unwrap();
        } else if user.online {
            send.unbounded_send(UiMessage::UserConnected {
                identity: user.identity,
                name: user_name_or_identity(user),
            })
            .unwrap();
        }
    }
}

fn user_name_or_identity(user: &User) -> String {
    user.name
        .clone()
        .unwrap_or_else(|| user.identity.to_abbreviated_hex().to_string())
}

// ## Notify about updated users

/// Our `User::on_update` callback:
/// print a notification about name and status changes.
fn on_user_updated(send: UiSend) -> impl FnMut(&User, &User, Option<&ReducerEvent>) + Send + 'static {
    move |old, new, _| {
        if new.identity == identity().unwrap() {
            if old.name != new.name {
                send.unbounded_send(UiMessage::SetOwnName {
                    name: user_name_or_identity(new),
                })
                .unwrap();
            }
        } else {
            if old.name != new.name {
                send.unbounded_send(UiMessage::SetName {
                    identity: new.identity,
                    new_name: user_name_or_identity(new),
                })
                .unwrap();
            }
            if old.online && !new.online {
                send.unbounded_send(UiMessage::UserDisconnected { identity: new.identity })
                    .unwrap();
            }
            if !old.online && new.online {
                send.unbounded_send(UiMessage::UserConnected {
                    identity: new.identity,
                    name: user_name_or_identity(new),
                })
                .unwrap();
            }
        }
    }
}

// ## Display incoming messages

/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(send: UiSend) -> impl FnMut(&Message, Option<&ReducerEvent>) + Send + 'static {
    move |message, reducer_event| {
        if reducer_event.is_some() {
            print_message(&send, message);
        }
    }
}

fn print_message(send: &UiSend, message: &Message) {
    let sender = User::find_by_identity(message.sender)
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    send.unbounded_send(UiMessage::Message {
        sender_name: sender,
        sender_identity: message.sender,
        text: message.text.clone(),
    })
    .unwrap();
}

// ## Print message backlog

/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
fn on_sub_applied(send: UiSend) -> impl FnMut() + Send + 'static {
    move || {
        let mut messages = Message::iter().collect::<Vec<_>>();
        messages.sort_by_key(|m| m.sent);
        for message in messages {
            print_message(&send, &message);
        }
    }
}

// ## Warn if set_name failed

/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(send: UiSend) -> impl FnMut(&Identity, Option<Address>, &Status, &String) {
    move |_sender_id, _sender_addr, status, name| {
        if let Status::Failed(err) = status {
            send.unbounded_send(UiMessage::NameRejected {
                current_name: user_name_or_identity(&User::find_by_identity(identity().unwrap()).unwrap()),
                rejected_name: name.clone(),
                reason: err.clone(),
            })
            .unwrap();
        }
    }
}

// ## Warn if a message was rejected

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(send: UiSend) -> impl FnMut(&Identity, Option<Address>, &Status, &String) {
    move |_sender_id, _sender_addr, status, text| {
        if let Status::Failed(err) = status {
            send.unbounded_send(UiMessage::MessageRejected {
                rejected_message: text.clone(),
                reason: err.clone(),
            })
            .unwrap();
        }
    }
}

// ## Dialog and exit when disconnected

/// Our `on_disconnect` callback: show a dialog, then exit the process
fn on_disconnected(send: UiSend) -> impl FnMut() {
    move || {
        send.unbounded_send(UiMessage::ConnectionClosed).unwrap();
    }
}

// # Connect to the database

/// The URL of the SpacetimeDB instance hosting our chat module.
const SPACETIMEDB_URI: &str = "http://localhost:3000";

/// The module name we chose when we published our module.
const DB_NAME: &str = "chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() {
    connect(
        SPACETIMEDB_URI,
        DB_NAME,
        load_credentials(CREDS_DIR).expect("Error reading stored credentials"),
        None,
    )
    .expect("Failed to connect");
}

// # Subscribe to queries

/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables() {
    subscribe(&["SELECT * FROM User;", "SELECT * FROM Message;"]).unwrap();
}

// # Construct the user interface

const MESSAGES_VIEW_NAME: &str = "Messages";
const NEW_MESSAGE_VIEW_NAME: &str = "NewMessage";
const ONLINE_USERS_VIEW_NAME: &str = "OnlineUsers";
const SET_NAME_VIEW_NAME: &str = "SetName";
const MESSAGE_SENDER_VIEW_NAME: &str = "MessageSender";

const USERS_WIDTH: usize = 20;

/// Construct our TUI.
///
/// Our UI will have 3 parts:
/// - The messages view, which displays incoming and sent messages.
/// - The online users view, which lists all currently-online remote users.
/// - The input bar, which allows the user to set their name and to send messages.
fn make_ui() -> CursiveRunnable {
    let mut siv = cursive::default();

    siv.add_layer(
        LinearLayout::horizontal()
            .child(PaddedView::lrtb(
                1,
                1,
                1,
                1,
                LinearLayout::vertical()
                    .child(make_messages_view())
                    .child(make_input_bar()),
            ))
            .child(PaddedView::lrtb(0, 1, 1, 1, make_online_users_view()))
            .full_screen(),
    );

    siv
}

/// Construct the messages view, which displays incoming and sent messages.
fn make_messages_view() -> impl View {
    ScrollView::new(LinearLayout::vertical().with_name(MESSAGES_VIEW_NAME))
        .scroll_strategy(cursive::view::ScrollStrategy::StickToBottom)
        .full_screen()
}

/// Construct the input bar, which allows the user to set their name and to send messages.
///
/// The input bar has two inputs:
/// - The set name view, an editable text box which shows the user's name.
///   The user can change their name by typing in it and hitting enter.
/// - The new message view, an editable text box where the user can type a message,
///   then hit enter to send it.
fn make_input_bar() -> impl View {
    LinearLayout::horizontal()
        .child(make_set_name_view())
        .child(TextView::new(": "))
        .child(make_new_message_view())
}

/// Construct the new message view, where the user can type new messages.
fn make_new_message_view() -> impl View {
    EditView::new()
        .on_submit(ui_send_message)
        .with_name(NEW_MESSAGE_VIEW_NAME)
        .full_width()
}

/// The UI callback on the send message view:
/// invoke the `send_message` reducer, then clear the input box.
fn ui_send_message(siv: &mut Cursive, text: &str) {
    send_message(text.to_string());
    siv.call_on_name(NEW_MESSAGE_VIEW_NAME, |new_message: &mut EditView| {
        new_message.set_content("");
    });
}

/// Construct the set name view, which displays the user's name.
/// The user can type into it and press enter to set their name.
fn make_set_name_view() -> impl View {
    EditView::new()
        .on_submit(ui_set_name)
        .with_name(SET_NAME_VIEW_NAME)
        .fixed_width(USERS_WIDTH)
}

/// The UI callback on the set name view:
/// invoke the `set_name` reducer.
/// Leave the new name in the input box.
fn ui_set_name(_siv: &mut Cursive, name: &str) {
    set_name(name.to_string());
}

/// Construct the online users view, which lists all the online remote users.
fn make_online_users_view() -> impl View {
    ScrollView::new(LinearLayout::vertical().with_name(ONLINE_USERS_VIEW_NAME))
        .scroll_strategy(cursive::view::ScrollStrategy::KeepRow)
        .full_height()
        .fixed_width(USERS_WIDTH)
}

// # Run our user interface

/// Run the Cursive TUI.
///
/// Because we need to push server-driven events into the UI,
/// we can't use Cursive's built-in event loop.
/// Instead, we need our own loop which asks Cursive to process user inputs,
/// then processes server-driven events from the `UiMessage` channel,
/// then re-draws the UI if necessary.
fn user_input_loop(siv: CursiveRunnable, mut recv: UiRecv) {
    let mut siv = siv.into_runner();
    siv.refresh();

    'per_frame: loop {
        siv.step();
        if !siv.is_running() {
            break 'per_frame;
        }

        // Will be true if any processed `UiMessage` causes any element to be redrawn.
        // We'll only re-draw the UI if a change happened.
        let mut needs_refresh = false;

        'process_message: loop {
            match recv.try_next() {
                // futures-channel returns `Err` to denote "queue is empty,"
                // so we've processed all messages for this frame.
                Err(_) => break 'process_message,

                // futures-channel returns `Ok(None)` to denote "channel is closed,"
                // so exit the UI loop.
                Ok(None) => break 'per_frame,

                // Process the next `UiMessage` and set `needs_refresh`.
                Ok(Some(message)) => {
                    needs_refresh |= process_ui_message(&mut siv, message);
                }
            }
        }

        // If any UI element changed, re-draw the UI.
        if needs_refresh {
            siv.refresh();
        }
    }
}

/// Update past messages sent by `identity` to change their sender name to `new_name`.
fn rename_message_senders(identity: &Identity, new_name: &str, siv: &mut CursiveRunner<CursiveRunnable>) -> bool {
    // Like in the main UI loop, we'll track if anything has changed in the UI,
    // i.e. if any messages had their sender renamed.
    let mut needs_update = false;

    siv.call_on_name(MESSAGES_VIEW_NAME, |messages: &mut LinearLayout| {
        // For each message sent by `identity`,
        messages.call_on_all(
            &cursive::view::Selector::Name(&identity.to_hex()),
            |message: &mut LinearLayout| {
                needs_update = true;
                // change the sender to the new name.
                message.call_on_name(MESSAGE_SENDER_VIEW_NAME, |name: &mut TextView| {
                    name.set_content(format!("{}: ", new_name));
                });
            },
        );
    });

    needs_update
}

/// Update the set name view to display the user's name.
fn set_own_name(siv: &mut CursiveRunner<CursiveRunnable>, name: String) {
    siv.call_on_name(SET_NAME_VIEW_NAME, |set_name: &mut EditView| {
        set_name.set_content(name.clone());
    });
}

/// Process a single `UiMessage`.
///
/// Returns true if the `UiMessage` caused any UI element to be updated.
fn process_ui_message(siv: &mut CursiveRunner<CursiveRunnable>, message: UiMessage) -> bool {
    match message {
        // When a new user connects, add them to the online users view.
        UiMessage::UserConnected { identity, name } => {
            siv.call_on_name(ONLINE_USERS_VIEW_NAME, |online_users: &mut LinearLayout| {
                online_users.add_child(
                    TextView::new(name)
                        // Tag their entry in the online users view with their identity,
                        // so we can find it later in the `UserDisconnected` and `SetName` branches.
                        .with_name(identity.to_hex().to_string()),
                );
                true
            })
        }

        // When a user disconnects, remove them from the online users view.
        UiMessage::UserDisconnected { identity } => {
            siv.call_on_name(ONLINE_USERS_VIEW_NAME, |online_users: &mut LinearLayout| {
                online_users
                    // Look up their entry in the online users view by their identity.
                    .find_child_from_name(&identity.to_hex())
                    .map(|idx| {
                        online_users.remove_child(idx);
                        true
                    })
                    .unwrap_or(false)
            })
        }

        // When our own name successfully changes,
        // update the set name view to show it,
        // and update our past messages.
        UiMessage::SetOwnName { name } => {
            set_own_name(siv, name.clone());
            // Look up our past messages by our identity.
            rename_message_senders(&identity().unwrap(), &name, siv);
            Some(true)
        }
        // When someone else updates their name,
        // change it in the online users view,
        // and update their past messages.
        UiMessage::SetName { identity, new_name } => {
            siv.call_on_name(ONLINE_USERS_VIEW_NAME, |online_users: &mut LinearLayout| {
                // Look up their entry in the online users view by their identity.
                online_users.call_on_name(&identity.to_hex(), |view: &mut TextView| {
                    view.set_content(new_name.clone());
                });
            });
            // Look up their past messages by their identity.
            rename_message_senders(&identity, &new_name, siv);

            Some(true)
        }

        // When we receive a new message, add it to the messages view.
        UiMessage::Message {
            sender_name,
            sender_identity,
            text,
        } => siv.call_on_name(MESSAGES_VIEW_NAME, |messages: &mut LinearLayout| {
            messages.add_child(
                LinearLayout::horizontal()
                    .child(
                        TextView::new(format!("{}: ", sender_name))
                            // Tag the sender part with `MESSAGE_SENDER_VIEW_NAME`,
                            // so that `rename_message_senders` can find it.
                            .with_name(MESSAGE_SENDER_VIEW_NAME),
                    )
                    .child(TextView::new(text))
                    // Tag the message with the sender's identity,
                    // so that `rename_message_senders` can find it.
                    .with_name(sender_identity.to_hex().to_string()),
            );
            true
        }),

        // When a message we sent is rejected by the server,
        // display a dialog with the offending message and the rejection reason.
        UiMessage::MessageRejected {
            rejected_message,
            reason,
        } => {
            siv.add_layer(
                Dialog::around(
                    LinearLayout::vertical()
                        .child(TextView::new("Failed to send message."))
                        .child(TextView::new(reason))
                        .child(TextView::new(rejected_message)),
                )
                .dismiss_button("Ok"),
            );
            Some(true)
        }

        // When our new name is rejected by the server,
        // display a dialog with the offending message and the rejection reason.
        UiMessage::NameRejected {
            current_name,
            rejected_name,
            reason,
        } => {
            set_own_name(siv, current_name);
            siv.add_layer(
                Dialog::around(
                    LinearLayout::vertical()
                        .child(TextView::new("Failed to set name."))
                        .child(TextView::new(reason))
                        .child(TextView::new(rejected_name)),
                )
                .dismiss_button("Ok"),
            );
            Some(true)
        }

        UiMessage::ConnectionClosed => {
            siv.add_layer(
                Dialog::around(TextView::new("Connection closed.")).button("Exit", |_| std::process::exit(0)),
            );
            Some(true)
        }
    }
    .unwrap_or(false)
}
