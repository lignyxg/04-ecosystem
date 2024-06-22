use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use axum::extract::State;
use axum::routing::{get, patch};
use axum::{Json, Router};
use derive_builder::Builder;
use derive_more::{From, Into};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::{RandomIdGenerator, Tracer};
use opentelemetry_sdk::{trace, Resource};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::metadata::LevelFilter;
use tracing::{info, instrument};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, Layer};

#[derive(Debug, Clone, Serialize, Builder)]
pub struct User {
    #[builder(setter(into))]
    name: String,
    #[builder(setter(into))]
    age: Age,
    #[builder(default = "Vec::new()", setter(each(name = "skill", into)))]
    skills: Vec<String>,
}

#[derive(Debug, From, Into, Serialize, Deserialize, Clone)]
pub struct Age(u8);

#[derive(Debug, Clone, Deserialize)]
pub struct UserUpdate {
    age: Option<Age>,
    skills: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let console = fmt::Layer::new()
        .with_span_events(FmtSpan::CLOSE)
        .pretty()
        .with_filter(LevelFilter::INFO);

    let tracer = init_tracer()?;
    let open_telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(console)
        .with(open_telemetry)
        .init();

    let addr = "0.0.0.0:8081";
    let listener = TcpListener::bind(addr).await?;

    info!("Listening on: {}", addr);

    let user = UserBuilder::default()
        .name("Alice")
        .age(26)
        .skill("programming")
        .skill("debug")
        .build()
        .map_err(|e| anyhow!(e.to_string()))?;

    let user = Arc::new(Mutex::new(user));

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/", patch(update_handler))
        .with_state(user);
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

#[instrument]
async fn index_handler(State(user): State<Arc<Mutex<User>>>) -> Json<User> {
    user.lock().unwrap().clone().into()
}

#[instrument]
async fn update_handler(
    State(user): State<Arc<Mutex<User>>>,
    Json(user_update): Json<UserUpdate>,
) -> Json<User> {
    let mut gard = user.lock().unwrap();
    if let Some(age) = user_update.age {
        gard.age = age;
    }
    if let Some(skills) = user_update.skills {
        gard.skills = skills;
    }
    gard.clone().into()
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
                    "axum_serde",
                )])),
        )
        .install_batch(Tokio)?;
    Ok(tracer)
}
