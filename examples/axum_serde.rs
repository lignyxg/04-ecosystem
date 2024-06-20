use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use axum::extract::State;
use axum::routing::{get, patch};
use axum::{Json, Router};
use derive_builder::Builder;
use derive_more::{From, Into};
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

    tracing_subscriber::registry().with(console).init();

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
