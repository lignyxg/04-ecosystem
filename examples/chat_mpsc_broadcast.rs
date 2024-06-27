use std::fmt::{Display, Formatter};
use std::sync::Arc;

use anyhow::anyhow;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug)]
enum Message {
    UserJoin(String),
    UserLeft(String),
    Chat { user_name: String, content: String },
}

impl Message {
    fn chat(user_name: String, content: String) -> Self {
        Self::Chat { user_name, content }
    }
    fn user_join(user_name: String) -> Self {
        Self::UserJoin(user_name)
    }
    fn user_left(user_name: String) -> Self {
        Self::UserLeft(user_name)
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::UserJoin(name) => write!(f, "{} joined the chat.", name),
            Message::UserLeft(name) => write!(f, "{} left the chat.", name),
            Message::Chat { user_name, content } => write!(f, "{}:{}", user_name, content),
        }
    }
}

struct MessageBus {
    tx: Sender<Arc<Message>>,
}

impl MessageBus {
    fn new() -> Self {
        let (tx, _) = channel(512);
        Self { tx }
    }

    fn get_sender(&self) -> Sender<Arc<Message>> {
        self.tx.clone()
    }

    fn get_receiver(&self) -> Receiver<Arc<Message>> {
        self.tx.subscribe()
    }
}

async fn forward_to_client(
    mut rx: Receiver<Arc<Message>>,
    mut stream_sender: SplitSink<Framed<TcpStream, LinesCodec>, String>,
    client_name: String,
) -> anyhow::Result<()> {
    loop {
        match rx.recv().await {
            Ok(m) => {
                match m.as_ref() {
                    Message::UserLeft(left) if left.eq(&client_name) => {
                        stream_sender.send("Bye!".to_string()).await?;
                        break;
                    }
                    Message::UserJoin(join) if join.eq(&client_name) => {
                        stream_sender
                            .send(format!("Welcome {}!", client_name))
                            .await?;
                        continue;
                    }
                    Message::Chat { user_name, .. } if user_name.eq(&client_name) => continue,
                    _ => {}
                }
                if let Err(e) = stream_sender.send(m.to_string()).await {
                    warn!("error sending message to client: {}", e);
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {
                warn!("message lagged.");
            }
            Err(e) => {
                warn!("error receive message: {}", e);
                break;
            }
        }
    }
    Ok(())
}

async fn handle_client(
    stream: TcpStream,
    tx: Sender<Arc<Message>>,
    rx: Receiver<Arc<Message>>,
) -> anyhow::Result<()> {
    let mut framed = Framed::new(stream, LinesCodec::new());
    framed.send("Please enter your name:").await?;
    let Some(Ok(user_name)) = framed.next().await else {
        error!("error read user_name");
        return Err(anyhow!("error read user_name"));
    };

    info!("{} joined the chat.", user_name);

    let msg = Message::user_join(user_name.clone());
    tx.send(Arc::new(msg))?;

    let (stream_sender, mut stream_receiver) = framed.split();

    let cloned_name = user_name.clone();
    tokio::spawn(async move {
        forward_to_client(rx, stream_sender, cloned_name).await?;
        Ok::<(), anyhow::Error>(())
    });

    while let Some(line) = stream_receiver.next().await {
        match line {
            Ok(m) => {
                let msg = Message::chat(user_name.clone(), m);
                tx.send(Arc::new(msg))?;
            }
            Err(e) => {
                warn!("can not read line: {}", e);
                let msg = Message::user_left(user_name.clone());
                tx.send(Arc::new(msg))?;
                break;
            }
        };
    }

    info!("{} left the chat.", user_name);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let layer = tracing_subscriber::fmt::layer().pretty();
    tracing_subscriber::registry().with(layer).init();

    let addr = "0.0.0.0:8088";
    let listener = TcpListener::bind(addr).await?;
    info!("Start chat server, listening on {}", addr);

    let bus = MessageBus::new();
    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Accepted connection from {}", addr);
        let tx = bus.get_sender();
        let rx = bus.get_receiver();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, tx, rx).await {
                warn!("error handle client {}: {}", addr, e);
            }
        });
    }
}
