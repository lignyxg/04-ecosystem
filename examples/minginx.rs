use std::sync::Arc;

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{RandomIdGenerator, Tracer};
use opentelemetry_sdk::{trace, Resource};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, instrument, warn};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer};

#[derive(Debug)]
struct Config {
    listen_addr: String,
    upstream_addr: String,
}

impl Config {
    pub fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8082".to_string(),
            upstream_addr: "0.0.0.0:8081".to_string(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let console = fmt::Layer::new().pretty().with_filter(LevelFilter::INFO);

    let tracer = init_tracer()?;
    let open_telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(console)
        .with(open_telemetry)
        .init();

    let config = Config::default();
    let config = Arc::new(config);
    info!("upstream: {}", config.upstream_addr);
    info!("listen: {}", config.listen_addr);

    let listener = TcpListener::bind(&config.listen_addr).await?;

    loop {
        let (client, addr) = listener.accept().await?;
        info!("Accepted connection: {}", addr);
        let cloned_config = Arc::clone(&config);
        tokio::spawn(async move {
            let upstream = TcpStream::connect(&cloned_config.upstream_addr).await?;
            proxy(client, upstream).await;
            Ok::<(), anyhow::Error>(())
        });
    }
}

#[instrument]
async fn proxy(mut client: TcpStream, mut upstream: TcpStream) {
    let (mut client_readr, mut client_writer) = client.split();
    let (mut upstream_readr, mut upstream_writer) = upstream.split();

    let client_to_upstream = tokio::io::copy(&mut client_readr, &mut upstream_writer);
    let upstream_to_client = tokio::io::copy(&mut upstream_readr, &mut client_writer);

    if let Err(e) = tokio::try_join!(client_to_upstream, upstream_to_client) {
        warn!("error: {}", e);
    }
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
                    "mingnix",
                )])),
        )
        .install_batch(Tokio)?;
    Ok(tracer)
}
