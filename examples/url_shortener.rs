use axum::extract::{Path, State};
use axum::http::header::LOCATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{debug_handler, Json, Router};
use log::warn;
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use sqlx::{Error, PgPool};
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Error, Debug)]
enum AppError {
    #[error("{0}")]
    DBError(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let resp = match self {
            AppError::DBError(err) => match err {
                Error::Configuration(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Error occurred while parsing a connection string.".to_string(),
                ),
                Error::Io(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Error communicating with the database backend.".to_string(),
                ),
                Error::Protocol(_) => (
                    StatusCode::BAD_REQUEST,
                    "Unexpected or invalid data encountered.".to_string(),
                ),
                Error::RowNotFound => (StatusCode::NOT_FOUND, "No data found.".to_string()),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            },
        };
        resp.into_response()
    }
}

#[derive(Debug, Deserialize)]
struct ShortenReq {
    url: String,
}

#[derive(Debug, Serialize)]
struct ShortenResp {
    url: String,
}

#[derive(Debug, Default, sqlx::FromRow)]
#[sqlx(default)]
struct UrlRecord {
    id: String,
    url: String,
}

#[derive(Debug, Clone)]
struct AppState {
    db: PgPool,
}

impl AppState {
    async fn try_new(url: &str) -> anyhow::Result<Self, AppError> {
        let db = PgPool::connect(url).await?;
        let sql = r#"CREATE TABLE IF NOT EXISTS urls (
            id CHAR(6) PRIMARY KEY,
            url TEXT NOT NULL UNIQUE
        )"#;
        sqlx::query(sql).execute(&db).await?;
        Ok(Self { db })
    }

    async fn shorten(&self, url: String) -> anyhow::Result<String, AppError> {
        let sql = "INSERT INTO urls(id, url) VALUES($1, $2) ON CONFLICT(url) \
        DO UPDATE SET url=EXCLUDED.url RETURNING id";
        let mut id = nanoid!(6);
        let url = Arc::new(url);
        let url_cloned = url.clone();
        loop {
            let ret: Result<UrlRecord, Error> = sqlx::query_as(sql)
                .bind(id.clone())
                .bind(url_cloned.as_str())
                .fetch_one(&self.db)
                .await;
            match ret {
                Ok(record) => {
                    info!("successful, id: {}", record.id);
                    return Ok(record.id);
                }
                Err(e) => {
                    warn!("duplicate id generated({}): {}", id, e);
                    id = nanoid!(6); // regenerate id
                }
            }
        }
    }

    async fn get_url(&self, id: String) -> anyhow::Result<String, AppError> {
        let record: UrlRecord = sqlx::query_as("SELECT * FROM urls WHERE id = $1")
            .bind(id)
            .fetch_one(&self.db)
            .await?;

        Ok(record.url)
    }
}

const LISTEN_ADDR: &str = "localhost:9898";
const DB_CONN: &str = "postgres://guannan:postgres@localhost:5432/shortener";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let layer = tracing_subscriber::fmt::layer().pretty();
    tracing_subscriber::registry().with(layer).init();

    let listener = TcpListener::bind(LISTEN_ADDR).await?;

    let app_state = AppState::try_new(DB_CONN).await?;

    let app = Router::new()
        .route("/", post(shorten))
        .route("/:id", get(redirect))
        .with_state(app_state);
    info!("Starting server on {}", LISTEN_ADDR);
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

#[debug_handler]
async fn redirect(
    Path(id): Path<String>,
    State(pg): State<AppState>,
) -> anyhow::Result<impl IntoResponse, AppError> {
    let url = pg.get_url(id).await?;
    let mut header = HeaderMap::new();
    header.insert(LOCATION, url.parse().unwrap());
    Ok((StatusCode::PERMANENT_REDIRECT, header))
}

#[debug_handler]
async fn shorten(
    State(pg): State<AppState>,
    Json(req): Json<ShortenReq>,
) -> Result<impl IntoResponse, AppError> {
    let id = pg.shorten(req.url).await?;
    let url = format!("http://{}/{}", LISTEN_ADDR, id);
    let body = Json(ShortenResp { url });
    Ok((StatusCode::CREATED, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db() -> anyhow::Result<()> {
        let pg = AppState::try_new(DB_CONN).await?;
        let sql = "INSERT INTO urls(id, url) VALUES($1, $2) ON CONFLICT(url) \
        DO UPDATE SET url=EXCLUDED.url RETURNING id";
        let url = "https://www.baidu.com";
        let mut id = nanoid!(6);
        let ret: UrlRecord = sqlx::query_as(sql)
            .bind(id.clone())
            .bind(url.to_string())
            .fetch_one(&pg.db)
            .await?;
        eprintln!("ret: {:?}", ret);

        // insert the same url
        eprintln!("insert the same url");
        let id2 = nanoid!(6);
        let ret: UrlRecord = sqlx::query_as(sql)
            .bind(id2.clone())
            .bind(url.to_string())
            .fetch_one(&pg.db)
            .await?;
        eprintln!("ret: {:?}", ret);

        // insert the same id
        eprintln!("insert the same id");
        let url2 = "https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html";
        let mut count = 1u64;

        loop {
            let ret: Result<UrlRecord, Error> = sqlx::query_as(sql)
                .bind(id.clone())
                .bind(url2.to_string())
                .fetch_one(&pg.db)
                .await;
            match ret {
                Ok(record) => {
                    eprintln!("successful, id: {}", record.id);
                    break;
                }
                Err(e) => {
                    if let Error::Database(derr) = e {
                        eprintln!("database error: {}", derr);
                        if count > 3 {
                            id = nanoid!(6);
                        }
                        eprintln!("tried {} time(s)...", count);
                        count += 1;
                        continue;
                    }
                    eprintln!("another database error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
