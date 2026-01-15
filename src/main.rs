//! OpenSASE Payments - Self-hosted Payment Processing

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use validator::Validate;

// =============================================================================
// Domain Models
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: Uuid,
    pub reference: String,
    pub amount: Decimal,
    pub currency: String,
    pub status: String,
    pub transaction_type: String,
    pub customer_id: Option<Uuid>,
    pub customer_email: Option<String>,
    pub payment_method: Option<String>,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Wallet {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub balance: Decimal,
    pub currency: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentMethod {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub method_type: String,
    pub provider: String,
    pub token: String,
    pub last_four: Option<String>,
    pub brand: Option<String>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Refund {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub amount: Decimal,
    pub reason: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// Application State
// =============================================================================

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub nats: Option<async_nats::Client>,
    pub config: Arc<Config>,
}

#[derive(Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub nats_url: Option<String>,
    pub paystack_secret: Option<String>,
    pub flutterwave_secret: Option<String>,
}

impl Config {
    fn from_env() -> Result<Self> {
        Ok(Config {
            port: std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8084),
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL required"),
            nats_url: std::env::var("NATS_URL").ok(),
            paystack_secret: std::env::var("PAYSTACK_SECRET_KEY").ok(),
            flutterwave_secret: std::env::var("FLUTTERWAVE_SECRET_KEY").ok(),
        })
    }
}

// =============================================================================
// Request/Response DTOs
// =============================================================================

#[derive(Debug, Deserialize, Validate)]
pub struct InitiatePaymentRequest {
    #[validate(range(min = 1))]
    pub amount: i64,
    pub currency: Option<String>,
    #[validate(email)]
    pub email: String,
    pub customer_id: Option<Uuid>,
    pub payment_method: Option<String>,
    pub callback_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct InitiatePaymentResponse {
    pub reference: String,
    pub authorization_url: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyPaymentRequest {
    pub reference: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RefundRequest {
    pub transaction_id: Uuid,
    #[validate(range(min = 1))]
    pub amount: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct WalletTopupRequest {
    pub customer_id: Uuid,
    #[validate(range(min = 1))]
    pub amount: i64,
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct TransferRequest {
    pub from_wallet_id: Uuid,
    pub to_wallet_id: Uuid,
    #[validate(range(min = 1))]
    pub amount: i64,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,opensase_payments=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting OpenSASE Payments...");

    let config = Config::from_env()?;
    let config = Arc::new(config);

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let nats = if let Some(ref url) = config.nats_url {
        async_nats::connect(url).await.ok()
    } else {
        None
    };

    let state = AppState { db, nats, config: config.clone() };
    let app = build_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("🚀 OpenSASE Payments listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api_routes())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/payments/initiate", post(initiate_payment))
        .route("/payments/verify", post(verify_payment))
        .route("/payments/webhook", post(webhook_handler))
        .route("/transactions", get(list_transactions))
        .route("/transactions/:id", get(get_transaction))
        .route("/refunds", post(create_refund).get(list_refunds))
        .route("/wallets", post(create_wallet).get(list_wallets))
        .route("/wallets/:id", get(get_wallet))
        .route("/wallets/:id/topup", post(topup_wallet))
        .route("/transfers", post(create_transfer))
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "opensase-payments",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// =============================================================================
// Payment Handlers
// =============================================================================

async fn initiate_payment(
    State(state): State<AppState>,
    Json(req): Json<InitiatePaymentRequest>,
) -> Result<Json<InitiatePaymentResponse>, (StatusCode, String)> {
    req.validate().map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let reference = format!("TXN-{}", Uuid::now_v7());
    let id = Uuid::now_v7();
    let amount = Decimal::new(req.amount, 2);

    sqlx::query(
        r#"INSERT INTO transactions (id, reference, amount, currency, status, transaction_type, customer_email, metadata, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'pending', 'payment', $5, $6, NOW(), NOW())"#
    )
    .bind(id)
    .bind(&reference)
    .bind(amount)
    .bind(req.currency.as_deref().unwrap_or("NGN"))
    .bind(&req.email)
    .bind(req.metadata.unwrap_or(serde_json::json!({})))
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // In production, integrate with Paystack/Flutterwave here
    let authorization_url = Some(format!("https://checkout.paystack.com/{}", reference));

    Ok(Json(InitiatePaymentResponse {
        reference,
        authorization_url,
        status: "pending".to_string(),
    }))
}

async fn verify_payment(
    State(state): State<AppState>,
    Json(req): Json<VerifyPaymentRequest>,
) -> Result<Json<Transaction>, (StatusCode, String)> {
    let txn = sqlx::query_as::<_, Transaction>(
        "SELECT * FROM transactions WHERE reference = $1"
    )
    .bind(&req.reference)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Transaction not found".to_string()))?;

    Ok(Json(txn))
}

async fn webhook_handler(
    State(_state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    tracing::info!("Webhook received: {:?}", payload);
    StatusCode::OK
}

async fn list_transactions(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<PaginatedResponse<Transaction>>, (StatusCode, String)> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let offset = ((page - 1) * per_page) as i64;

    let transactions = sqlx::query_as::<_, Transaction>(
        "SELECT * FROM transactions ORDER BY created_at DESC LIMIT $1 OFFSET $2"
    )
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transactions")
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(PaginatedResponse { data: transactions, total: total.0, page, per_page }))
}

async fn get_transaction(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Transaction>, (StatusCode, String)> {
    let txn = sqlx::query_as::<_, Transaction>("SELECT * FROM transactions WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Transaction not found".to_string()))?;

    Ok(Json(txn))
}

async fn create_refund(
    State(state): State<AppState>,
    Json(req): Json<RefundRequest>,
) -> Result<(StatusCode, Json<Refund>), (StatusCode, String)> {
    let id = Uuid::now_v7();
    let amount = Decimal::new(req.amount.unwrap_or(0), 2);

    let refund = sqlx::query_as::<_, Refund>(
        r#"INSERT INTO refunds (id, transaction_id, amount, reason, status, created_at)
           VALUES ($1, $2, $3, $4, 'pending', NOW()) RETURNING *"#
    )
    .bind(id)
    .bind(req.transaction_id)
    .bind(amount)
    .bind(&req.reason)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(refund)))
}

async fn list_refunds(
    State(state): State<AppState>,
) -> Result<Json<Vec<Refund>>, (StatusCode, String)> {
    let refunds = sqlx::query_as::<_, Refund>("SELECT * FROM refunds ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(refunds))
}

// =============================================================================
// Wallet Handlers
// =============================================================================

async fn create_wallet(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Wallet>), (StatusCode, String)> {
    let customer_id = req["customer_id"].as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or((StatusCode::BAD_REQUEST, "customer_id required".to_string()))?;

    let id = Uuid::now_v7();

    let wallet = sqlx::query_as::<_, Wallet>(
        r#"INSERT INTO wallets (id, customer_id, balance, currency, status, created_at, updated_at)
           VALUES ($1, $2, 0, 'NGN', 'active', NOW(), NOW()) RETURNING *"#
    )
    .bind(id)
    .bind(customer_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(wallet)))
}

async fn list_wallets(
    State(state): State<AppState>,
) -> Result<Json<Vec<Wallet>>, (StatusCode, String)> {
    let wallets = sqlx::query_as::<_, Wallet>("SELECT * FROM wallets ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(wallets))
}

async fn get_wallet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Wallet>, (StatusCode, String)> {
    let wallet = sqlx::query_as::<_, Wallet>("SELECT * FROM wallets WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;

    Ok(Json(wallet))
}

async fn topup_wallet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<WalletTopupRequest>,
) -> Result<Json<Wallet>, (StatusCode, String)> {
    let amount = Decimal::new(req.amount, 2);

    let wallet = sqlx::query_as::<_, Wallet>(
        "UPDATE wallets SET balance = balance + $1, updated_at = NOW() WHERE id = $2 RETURNING *"
    )
    .bind(amount)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;

    Ok(Json(wallet))
}

async fn create_transfer(
    State(state): State<AppState>,
    Json(req): Json<TransferRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let amount = Decimal::new(req.amount, 2);

    // Debit source wallet
    sqlx::query("UPDATE wallets SET balance = balance - $1 WHERE id = $2 AND balance >= $1")
        .bind(amount)
        .bind(req.from_wallet_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Credit destination wallet
    sqlx::query("UPDATE wallets SET balance = balance + $1 WHERE id = $2")
        .bind(amount)
        .bind(req.to_wallet_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "status": "completed",
        "amount": req.amount,
        "from": req.from_wallet_id,
        "to": req.to_wallet_id
    })))
}
