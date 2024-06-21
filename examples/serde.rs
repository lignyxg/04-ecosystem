use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    name: String,
    age: u8,
    dob: NaiveDate,
    skills: Vec<String>,
    state: WorkState,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "details")]
enum WorkState {
    Working(String),
    OnLeave(DateTime<Utc>),
    Terminated,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _state = WorkState::Working("Rust".to_string());
    let state1 = WorkState::OnLeave(Utc::now());
    let user = User {
        name: "Alice".to_string(),
        age: 30,
        dob: Default::default(),
        skills: vec!["Rust".to_string(), "Go".to_string()],
        state: state1,
    };

    let json = serde_json::to_string(&user)?;

    println!("{json}");

    Ok(())
}

// TODO: usage of serde_with::serde_as macro
