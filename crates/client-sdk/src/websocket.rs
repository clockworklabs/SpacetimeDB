use crate::identity::Credentials;
use anyhow::{bail, Result};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use futures_channel::mpsc;
use http::uri::{Parts, Uri};
use prost::Message as ProtobufMessage;
use spacetimedb_client_api_messages::client_api::Message;
use tokio::{net::TcpStream, runtime, task::JoinHandle};
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message as WebSocketMessage,
    MaybeTlsStream, WebSocketStream,
};

pub(crate) struct DbConnection {
    pub(crate) read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    pub(crate) write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, WebSocketMessage>,
}

fn make_uri<Host>(host: Host, db_name: &str) -> Result<Uri>
where
    Host: TryInto<Uri>,
    <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
{
    let host: Uri = host.try_into()?;
    let mut parts = Parts::try_from(host)?;
    match &parts.scheme {
        Some(s) => match s.as_str() {
            "ws" | "wss" => (),
            unknown_scheme => bail!("Unknown URI scheme {}", unknown_scheme),
        },
        None => parts.scheme = Some("ws".parse()?),
    }
    let mut path = if let Some(path_and_query) = parts.path_and_query {
        if let Some(query) = path_and_query.query() {
            bail!("Unexpected query {}", query);
        }
        path_and_query.path().to_string()
    } else {
        "/".to_string()
    };

    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str("database/subscribe/");
    path.push_str(db_name);
    parts.path_and_query = Some(path.parse()?);
    Ok(Uri::try_from(parts)?)
}

// Tungstenite doesn't offer an interface to specify a WebSocket protocol, which frankly
// seems like a pretty glaring omission in its API. In order to insert our own protocol
// header, we manually the `Request` constructed by
// `tungstenite::IntoClientRequest::into_client_request`.

// TODO: `core` uses [Hyper](https://docs.rs/hyper/latest/hyper/) as its HTTP library
//       rather than having Tungstenite manage its own connections. Should this library do
//       the same?

fn make_request<Host>(host: Host, db_name: &str, credentials: Option<&Credentials>) -> Result<http::Request<()>>
where
    Host: TryInto<Uri>,
    <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
{
    let uri = make_uri(host, db_name)?;
    println!("Uri: {:?}", uri);
    let mut req = IntoClientRequest::into_client_request(uri)?;
    request_insert_protocol_header(&mut req);
    request_insert_auth_header(&mut req, credentials);
    Ok(req)
}

fn request_add_header(req: &mut http::Request<()>, key: &'static str, val: http::header::HeaderValue) {
    let _prev = req.headers_mut().insert(key, val);
    debug_assert!(_prev.is_none(), "HttpRequest already had {:?} header {:?}", key, _prev,);
}

const PROTOCOL_HEADER_KEY: &str = "Sec-WebSocket-Protocol";
const PROTOCOL_HEADER_VALUE: &str = "v1.bin.spacetimedb";

fn request_insert_protocol_header(req: &mut http::Request<()>) {
    request_add_header(
        req,
        PROTOCOL_HEADER_KEY,
        http::header::HeaderValue::from_static(PROTOCOL_HEADER_VALUE),
    );
}

const AUTH_HEADER_KEY: &str = "Authorization";

fn request_insert_auth_header(req: &mut http::Request<()>, credentials: Option<&Credentials>) {
    // TODO: figure out how the token is supposed to be encoded in the request
    if let Some(Credentials { token, .. }) = credentials {
        request_add_header(
            req,
            AUTH_HEADER_KEY,
            token
                .string
                .clone()
                .try_into()
                .expect("Failed to convert token to http HeaderValue"),
        )
    };
}

impl DbConnection {
    pub(crate) async fn connect<Host>(host: Host, db_name: &str, credentials: Option<&Credentials>) -> Result<Self>
    where
        Host: TryInto<Uri>,
        <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let req = make_request(host, db_name, credentials)?;
        let (stream, _): (WebSocketStream<MaybeTlsStream<TcpStream>>, _) = connect_async(req).await?;
        let (write, read) = stream.split();
        Ok(DbConnection { write, read })
    }

    pub(crate) fn parse_response(bytes: &[u8]) -> Result<Message> {
        Ok(Message::decode(bytes)?)
    }

    pub(crate) fn encode_message(msg: Message) -> WebSocketMessage {
        WebSocketMessage::Binary(msg.encode_to_vec())
    }

    fn maybe_log_error<T, U: std::fmt::Debug>(cause: &str, res: std::result::Result<T, U>) {
        if let Err(e) = res {
            log::warn!("{}: {:?}", cause, e);
        }
    }

    async fn message_loop(
        mut self,
        incoming_messages: mpsc::UnboundedSender<Message>,
        mut outgoing_messages: mpsc::UnboundedReceiver<Message>,
    ) {
        loop {
            tokio::select! {
                Some(incoming) = self.read.next() => match incoming {
                    Err(e) => Self::maybe_log_error::<(), _>(
                        "Error reading message from read WebSocket stream",
                        Err(e),
                    ),

                    Ok(WebSocketMessage::Binary(bytes)) => {
                        match Self::parse_response(&bytes) {
                            Err(e) => Self::maybe_log_error::<(), _>(
                                "Error decoding WebSocketMessage::Binary payload",
                                Err(e),
                            ),
                            Ok(msg) => Self::maybe_log_error(
                                "Error sending decoded message to incoming_messages queue",
                                incoming_messages.unbounded_send(msg),
                            ),
                        }
                    }

                    Ok(WebSocketMessage::Ping(payload)) => Self::maybe_log_error(
                        "Error sending Pong in response to Ping",
                        self.write.send(WebSocketMessage::Pong(payload)).await,
                    ),

                    Ok(other) => log::warn!("Unexpected WebSocket message {:?}", other),
                },

                Some(outgoing) = outgoing_messages.next() => {
                    let msg = Self::encode_message(outgoing);
                    Self::maybe_log_error(
                        "Error sending outgoing message",
                        self.write.send(msg).await,
                    );
                },
            }
        }
    }

    pub(crate) fn spawn_message_loop(
        self,
        runtime: &runtime::Handle,
    ) -> (
        JoinHandle<()>,
        mpsc::UnboundedReceiver<Message>,
        mpsc::UnboundedSender<Message>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded();
        let (incoming_send, incoming_recv) = mpsc::unbounded();
        let handle = runtime.spawn(self.message_loop(incoming_send, outgoing_recv));
        (handle, incoming_recv, outgoing_send)
    }
}
