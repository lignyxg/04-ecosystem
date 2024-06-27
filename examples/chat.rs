use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::anyhow;
use dashmap::DashMap;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{error, info, warn};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer};

#[derive(Debug)]
struct Peer {
    name: String,
    stream: SplitSink<Framed<TcpStream, LinesCodec>, String>,
}

impl Peer {
    pub fn new(name: String, stream: SplitSink<Framed<TcpStream, LinesCodec>, String>) -> Self {
        Self { name, stream }
    }
}

#[derive(Debug)]
struct Message {
    username: String,
    content: String,
}

impl Message {
    fn new(username: String, content: String) -> Self {
        Self { username, content }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.username, self.content)
    }
}

#[derive(Debug, Default)]
struct Server {
    peers: DashMap<SocketAddr, Peer>,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn join(&self, addr: SocketAddr, peer: Peer) -> anyhow::Result<()> {
        let name = peer.name.clone();
        self.peers.insert(addr, peer);
        let msg = format!("{} joined the chat.", name);
        info!(msg);
        let msg = Message::new("Server".to_string(), msg);
        self.broadcast(addr, Arc::new(msg)).await?;
        Ok(())
    }

    pub async fn broadcast(&self, src_addr: SocketAddr, msg: Arc<Message>) -> anyhow::Result<()> {
        for mut peer in self.peers.iter_mut() {
            if peer.key().eq(&src_addr) {
                continue;
            }
            let msg = msg.clone();
            if let Err(e) = peer.stream.send(msg.to_string()).await {
                warn!("failed sending message to {}: {}", peer.key(), e);
                self.peers.remove(peer.key());
            }
        }

        Ok(())
    }

    pub async fn leave(&self, addr: SocketAddr) -> anyhow::Result<()> {
        let Some((_, peer)) = self.peers.remove(&addr) else {
            return Err(anyhow!("fail to remove peer({}) from global state.", addr));
        };
        let msg = format!("{} left the chat.", peer.name);

        info!(msg);
        let msg = Message::new("Server".to_string(), msg);
        self.broadcast(addr, Arc::new(msg)).await
    }
}

async fn handle_client(
    mut stream: Framed<TcpStream, LinesCodec>,
    addr: SocketAddr,
    server: Arc<Server>,
) -> anyhow::Result<()> {
    stream.send("Please enter your name:").await?;
    let Some(Ok(name)) = stream.next().await else {
        let err_msg = "failed to get username".to_string();
        error!(err_msg);
        return Err(anyhow!(err_msg));
    };

    let (writer, mut reader) = stream.split();
    let peer = Peer::new(name.clone(), writer);

    server.join(addr, peer).await?;

    while let Some(line) = reader.next().await {
        match line {
            Ok(msg) => {
                if !msg.is_empty() {
                    let msg = Message::new(name.clone(), msg);
                    server.broadcast(addr, Arc::new(msg)).await?;
                } else {
                    warn!("empty line");
                    continue;
                }
            }
            Err(e) => {
                warn!("error read line from {}: {}", addr, e);
                break;
            }
        }
    }

    server.leave(addr).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let layer = fmt::Layer::new().pretty().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let addr = "0.0.0.0:8088";
    let listener = TcpListener::bind(addr).await?;
    info!("Listening on {}.", addr);
    let server = Server::default();
    let server = Arc::new(server);
    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Accepted connection from {}", addr);
        let framed = Framed::new(stream, LinesCodec::default());
        let server_cloned = server.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(framed, addr, server_cloned).await {
                error!("error handle client {}: {}", addr, e);
            }
        });
    }
}
