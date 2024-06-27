use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use anyhow::anyhow;
use dashmap::DashMap;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug)]
enum Message {
    UserJoined {
        user_name: String,
        addr: SocketAddr,
        handle: Sender<Arc<Message>>,
    },
    UserLeft {
        user_name: String,
        addr: SocketAddr,
    },
    Chat {
        user_name: String,
        content: String,
    },
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::UserJoined { user_name, .. } => {
                write!(f, "{} has joined the chat.", user_name)
            }
            Message::UserLeft { user_name, .. } => {
                write!(f, "{} left the chat.", user_name)
            }
            Message::Chat { user_name, content } => {
                write!(f, "{}:{}", user_name, content)
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
struct State(DashMap<SocketAddr, Sender<Arc<Message>>>);

impl Deref for State {
    type Target = DashMap<SocketAddr, Sender<Arc<Message>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl State {
    async fn broadcast(&self, addr: SocketAddr, msg: Arc<Message>) {
        for peer in self.iter() {
            if peer.key().eq(&addr) {
                continue;
            }
            if let Err(e) = peer.value().send(msg.clone()).await {
                warn!("can not send to peer[{}]: {}", peer.key(), e);
                self.remove(peer.key());
            }
        }
    }
}

#[derive(Debug)]
struct Peer {
    user_name: String,
    addr: SocketAddr,
    /// all the other peers to receive message from client
    others: Arc<State>,
}

impl Peer {
    fn new(user_name: String, addr: SocketAddr, others: State) -> Self {
        Self {
            user_name,
            addr,
            others: Arc::new(others),
        }
    }

    /// forward message to client
    fn init(
        &self,
        mut notifier: Receiver<Arc<Message>>,
        mut stream_sender: SplitSink<Framed<TcpStream, LinesCodec>, String>,
    ) {
        let state = self.others.clone();

        tokio::spawn(async move {
            while let Some(msg) = notifier.recv().await {
                match msg.as_ref() {
                    Message::UserJoined { addr, handle, .. } => {
                        state.insert(*addr, handle.clone());
                    }
                    Message::UserLeft { addr, .. } => {
                        state.remove(addr);
                    }
                    Message::Chat { .. } => {}
                }
                if let Err(e) = stream_sender.send(msg.to_string()).await {
                    warn!("send message error: {}", e);
                    break;
                }
            }
        });
    }

    /// receive message from client, pass to other peers
    async fn receive(&self, mut stream_receiver: SplitStream<Framed<TcpStream, LinesCodec>>) {
        while let Some(frame) = stream_receiver.next().await {
            let content = match frame {
                Ok(m) => m,
                Err(e) => {
                    warn!("can not read line: {}", e);
                    break;
                }
            };

            let msg = Message::Chat {
                user_name: self.user_name.clone(),
                content,
            };
            self.others.broadcast(self.addr, Arc::new(msg)).await;
        }
    }
}

#[derive(Debug, Default)]
struct Registry {
    peers: State,
}

impl Registry {
    const MAX_MSG: usize = 128;
    /// get a peer and message faucet
    async fn register(&self, addr: SocketAddr, name: String) -> (Peer, Receiver<Arc<Message>>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<Arc<Message>>(Self::MAX_MSG);

        // user join message
        let msg = Message::UserJoined {
            user_name: name.clone(),
            addr,
            handle: tx.clone(),
        };
        let msg = Arc::new(msg);
        info!("{} has joined the chat.", name);
        let others = self.peers.clone();
        // notify all peers
        self.peers.broadcast(addr, msg.clone()).await;
        // register to registry
        self.peers.insert(addr, tx);

        let peer = Peer::new(name, addr, others);
        (peer, rx)
    }

    async fn cancel(&self, addr: SocketAddr, user_name: String) {
        self.peers.remove(&addr);
        info!("{} left the chat.", user_name);
        let msg = Arc::new(Message::UserLeft { user_name, addr });
        self.peers.broadcast(addr, msg.clone()).await;
    }
}

async fn handle_client(
    stream: TcpStream,
    addr: SocketAddr,
    registry: Arc<Registry>,
) -> anyhow::Result<()> {
    let mut framed = Framed::new(stream, LinesCodec::new());

    framed.send("Please enter your name:").await?;
    let Some(Ok(user_name)) = framed.next().await else {
        error!("error read user_name");
        return Err(anyhow!("error read user_name"));
    };

    let (peer, notifier) = registry.register(addr, user_name.clone()).await;

    let (stream_sender, stream_receiver) = framed.split();
    peer.init(notifier, stream_sender);
    peer.receive(stream_receiver).await;
    // drop(peer);
    registry.cancel(addr, user_name).await;
    info!("client log out.");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let layer = tracing_subscriber::fmt::layer().pretty();
    tracing_subscriber::registry().with(layer).init();

    let addr = "0.0.0.0:8088";
    let listener = TcpListener::bind(addr).await?;
    info!("Start chat server, listening on {}", addr);
    let registry = Registry::default();
    let registry = Arc::new(registry);

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Accepted connection from {}", addr);
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, addr, registry).await {
                warn!("error handle client {}: {}", addr, e);
            }
        });
    }
}
