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
                .unwrap_or_else(|_| "bouncarr=info,tower_http=info".into()),
        )
        .init();

    info!("Starting Bouncarr...");

    // Load configuration
    let config = Config::load()?;
    info!("Configuration loaded successfully");

    // Create Jellyfin client
    let jellyfin_client = JellyfinClient::new(
        config.jellyfin.clone(),
        config.server.request_timeout_seconds,
    )?;

    // Create JWT manager
    let jwt_manager = JwtManager::new(&config.security);

    // Create HTTP client with optional timeout
    let mut http_client_builder = reqwest::Client::builder();
    if config.server.request_timeout_seconds > 0 {
        http_client_builder = http_client_builder.timeout(std::time::Duration::from_secs(
            config.server.request_timeout_seconds as u64,
        ));
        info!(
            "HTTP client timeout set to {} seconds",
            config.server.request_timeout_seconds
        );
    } else {
        info!("HTTP client timeout disabled (no timeout)");
    }
    let http_client = http_client_builder.build()?;

    // Create shared application state
    let state = Arc::new(AppState {
        config: config.clone(),
        jellyfin_client,
        jwt_manager,
        http_client,
    });

    // Build the application router
    let app = build_router(state.clone());

    // Start the server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    // Graceful shutdown handler
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown...");
        },
        _ = terminate => {
            info!("Received SIGTERM, starting graceful shutdown...");
        },
    }
}

fn build_router(state: Arc<AppState>) -> Router {
    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(routes::health_check))
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
