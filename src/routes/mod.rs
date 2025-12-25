pub mod auth;
pub mod ui;

pub use auth::*;
pub use ui::*;

use axum::Json;
use serde_json::json;

pub async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "bouncarr"
    }))
}
