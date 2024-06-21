use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{RandomIdGenerator, Tracer};
use opentelemetry_sdk::{trace, Resource};
use tokio::runtime::Builder;
use tracing::{info, instrument};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Fixme: not work
fn main() -> anyhow::Result<()> {
    let tracer = init_tracer()?;
    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry().with(opentelemetry).init();

    let handle = std::thread::spawn(|| {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        rt.spawn(async { spawn1().await });

        rt.block_on(async { spawn2().await })
    });

    handle.join().unwrap();

    Ok(())
}

#[instrument]
async fn spawn1() {
    info!("Future 1");
    expensive_op();
    info!("Future 1 finish");
}

#[instrument]
async fn spawn2() {
    tokio::time::sleep(Duration::from_millis(100)).await;
    info!("Rust");
}

pub fn expensive_op() {
    std::thread::sleep(Duration::from_millis(500));
}

fn init_tracer() -> anyhow::Result<Tracer> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_trace_config(
            trace::config()
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![KeyValue::new(
                    "service.name",
                    "thread-runtime",
                )])),
        )
        .install_batch(Tokio)?;
    Ok(tracer)
}
