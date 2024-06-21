use std::thread;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel(42);
    let handler = worker(rx);

    tokio::spawn(async move {
        let mut i = 0;
        loop {
            i += 1;
            println!("sending task {}", i);
            tx.send(format!("task {}", i)).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });
    handler.await.join().unwrap();
    Ok(())
}

async fn worker(mut rx: Receiver<String>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Some(s) = rx.blocking_recv() {
            thread::sleep(Duration::from_millis(500));
            println!("received: {}", s);
        }
    })
}
