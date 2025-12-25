mod auth;
mod config;
mod error;
mod jellyfin;
mod proxy;
mod routes;

use crate::auth::JwtManager;
use crate::config::Config;
use crate::jellyfin::JellyfinClient;
use axum::{
    Router, middleware,
    response::Redirect,
    routing::{any, get, post},
};
use std::sync::Arc;
use tower_cookies::CookieManagerLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

pub struct AppState {
    config: Config,
    jellyfin_client: JellyfinClient,
    jwt_manager: JwtManager,
    http_client: reqwest::Client,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bouncarr=debug,tower_http=debug".into()),
        )
        .init();

    info!("Starting Bouncarr...");

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded successfully");

    // Create Jellyfin client
    let jellyfin_client = JellyfinClient::new(config.jellyfin.clone());

    // Create JWT manager
    let jwt_manager = JwtManager::new(&config.security);

    // Create shared application state
    let state = Arc::new(AppState {
        config: config.clone(),
        jellyfin_client,
        jwt_manager,
        http_client: reqwest::Client::new(),
    });

    // Build the application router
    let app = build_router(state.clone());

    // Start the server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router(state: Arc<AppState>) -> Router {
    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/bouncarr/login", get(routes::serve_login_page))
        .route("/bouncarr/api/auth/login", post(routes::login))
        .route("/bouncarr/api/auth/refresh", post(routes::refresh))
        .route("/bouncarr/api/auth/logout", post(routes::logout));

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        .route("/:app/*path", any(proxy::proxy_handler))
        .route("/:app/", any(proxy::proxy_handler))
        .route("/:app", any(proxy::proxy_handler))
        .route(
            "/",
            get(|| async { Redirect::permanent("/bouncarr/login") }),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    // Combine routes
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(CookieManagerLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
