use anyhow::{bail, ensure, Context, Result};
use futures::{SinkExt, Stream, StreamExt};
use reqwest::{
    header::{HeaderValue, AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL},
    Url,
};
use spacetimedb_lib::{container::exec::*, Identity};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message},
};

const NETWORK_WAIT: Duration = Duration::from_secs(10);
pub(super) enum Input {
    Data(Vec<u8>),
    Eof,
}
pub(super) struct Output {
    pub stream: OutputStream,
    pub bytes: Vec<u8>,
    pub completed: oneshot::Sender<Result<()>>,
}
pub(super) type OutputStream = spacetimedb_lib::container::exec::Stream;
pub(super) struct OutputSender {
    pub sender: mpsc::Sender<Output>,
    pub wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}
impl OutputSender {
    async fn send(&self, value: Output) -> Result<()> {
        self.sender.send(value).await.context("terminal output stopped")?;
        if let Some(wake) = &self.wake {
            wake();
        }
        Ok(())
    }
}
pub(super) struct Io {
    pub input: mpsc::Receiver<Input>,
    pub output: OutputSender,
    pub completion: oneshot::Receiver<Result<()>>,
}

pub(super) async fn run<S>(
    mut url: Url,
    authorization: HeaderValue,
    identity: Identity,
    start: ExecStart,
    io: &mut Io,
    mut signals: S,
) -> Result<u8>
where
    S: Stream<Item = Result<ClientControl>> + Unpin,
{
    start.validate()?;
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(),
        "invalid exec endpoint"
    );
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => bail!("invalid exec endpoint"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow::anyhow!("invalid exec endpoint"))?;
    url.query_pairs_mut()
        .append_pair("generation", &start.generation.to_string());
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| anyhow::anyhow!("invalid exec endpoint"))?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    request
        .headers_mut()
        .insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(SUBPROTOCOL));
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_CONTROL_BYTES))
        .max_frame_size(Some(MAX_CONTROL_BYTES))
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_CONTROL_BYTES + MAX_BINARY_BYTES + 1024);
    let (mut socket, response) =
        tokio::time::timeout(NETWORK_WAIT, connect_async_with_config(request, Some(config), true))
            .await
            .map_err(|_| anyhow::anyhow!("exec connection timed out; the command was not replayed"))?
            .map_err(|_| anyhow::anyhow!("exec connection was rejected or failed"))?;
    ensure!(
        response.headers().get_all(SEC_WEBSOCKET_PROTOCOL).iter().count() == 1
            && response
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .is_some_and(|value| value == SUBPROTOCOL),
        "server did not negotiate the exec protocol"
    );
    send(&mut socket, text(&ClientControl::Start(start.clone()))?).await?;
    tokio::time::timeout(NETWORK_WAIT, async {
        loop {
            match socket
                .next()
                .await
                .context("exec closed before Ready; process outcome is unknown")?
                .map_err(|_| anyhow::anyhow!("exec transport failed before Ready; process outcome is unknown"))?
            {
                Message::Text(value) => match ServerControl::decode(value.as_bytes())? {
                    ServerControl::Ready(ready) => {
                        ensure!(
                            ready.database_identity == identity
                                && ready.generation == start.generation
                                && ready.tty == start.terminal.is_some(),
                            "exec Ready did not match the selected instance"
                        );
                        return Ok(());
                    }
                    ServerControl::Error { .. } => bail!("container exec was rejected"),
                    _ => bail!("invalid exec response before Ready"),
                },
                Message::Ping(bytes) => send(&mut socket, Message::Pong(bytes)).await?,
                Message::Pong(_) => {}
                _ => bail!("invalid exec response before Ready"),
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("exec Ready timed out; process outcome is unknown"))??;

    let (mut sink, mut source) = socket.split();
    let (pong_tx, mut pong_rx) = mpsc::channel(1);
    let input = &mut io.input;
    let output = &io.output;
    let reader = async {
        while let Some(message) = source.next().await {
            match message.map_err(|_| anyhow::anyhow!("exec transport failed; process outcome is unknown"))? {
                Message::Binary(frame) => {
                    let (stream, bytes) = decode_output(&frame)?;
                    ensure!(
                        start.terminal.is_none() || stream == OutputStream::Stdout,
                        "unexpected PTY stderr channel"
                    );
                    let (completed, written) = oneshot::channel();
                    output
                        .send(Output {
                            stream,
                            bytes: bytes.to_vec(),
                            completed,
                        })
                        .await
                        .context("terminal output stopped")?;
                    written.await.context("terminal output stopped")??;
                }
                Message::Text(value) => match ServerControl::decode(value.as_bytes())? {
                    ServerControl::Exit { exit_code } => {
                        return u8::try_from(exit_code).context("invalid observed exec exit code")
                    }
                    ServerControl::Error { .. } => bail!("container exec failed; process outcome is unknown"),
                    ServerControl::Ready(_) => bail!("duplicate exec Ready"),
                },
                Message::Ping(bytes) => pong_tx.send(bytes).await.context("exec writer stopped")?,
                Message::Pong(_) => {}
                _ => bail!("exec closed without observed exit; process outcome is unknown"),
            }
        }
        bail!("exec closed without observed exit; process outcome is unknown")
    };
    let writer = async {
        let mut stdin_open = start.stdin;
        loop {
            let message = tokio::select! {
                value = input.recv(), if stdin_open => match value.context("terminal input stopped")? {
                    Input::Data(bytes) => Message::Binary(encode_data(OutputStream::Stdin, &bytes)?.into()),
                    Input::Eof => { stdin_open = false; text(&ClientControl::StdinEof)? },
                },
                value = signals.next() => text(&value.context("terminal signals stopped")??)?,
                Some(bytes) = pong_rx.recv() => Message::Pong(bytes),
            };
            send(&mut sink, message).await?;
        }
        #[allow(unreachable_code)]
        Ok::<u8, anyhow::Error>(0)
    };
    // These are owned, inline futures, not spawned tasks. The losing future and
    // both socket halves are dropped before the caller joins its terminal worker.
    tokio::select! {
        result = reader => result,
        result = writer => result,
        result = &mut io.completion => {
            result.context("terminal worker stopped")??;
            bail!("terminal worker stopped before observed exec exit")
        }
    }
}

fn text(value: &ClientControl) -> Result<Message> {
    value.validate()?;
    Ok(Message::Text(serde_json::to_string(value)?.into()))
}
async fn send<S>(sink: &mut S, message: Message) -> Result<()>
where
    S: futures::Sink<Message> + Unpin,
{
    tokio::time::timeout(NETWORK_WAIT, sink.send(message))
        .await
        .map_err(|_| anyhow::anyhow!("exec write timed out; process outcome is unknown"))?
        .map_err(|_| anyhow::anyhow!("exec write failed; process outcome is unknown"))
}
