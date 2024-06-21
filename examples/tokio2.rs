use std::thread::JoinHandle;
use std::time::Duration;

use tokio::runtime::Builder;

fn main() -> anyhow::Result<()> {
    let handle = thread_tokio_rt();

    handle.join().unwrap();

    Ok(())
}

fn thread_tokio_rt() -> JoinHandle<()> {
    std::thread::spawn(|| {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        rt.spawn(async {
            println!("Future 1");
            expensive_op();
            println!("Future 1 finish");
        });

        rt.block_on(async {
            println!("golang");
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("Rust");
        })
    })
}

pub fn expensive_op() {
    std::thread::sleep(Duration::from_millis(500));
}
