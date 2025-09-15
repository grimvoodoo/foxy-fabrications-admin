use anyhow::Result;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{delete, get, post},
    Extension, Router,
};
use axum_login::{
    tower_sessions::{MemoryStore, SessionManagerLayer}, AuthManagerLayerBuilder,
};
use dotenv::dotenv;
use mongodb::{Client, Collection};
use std::env;
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Import modules
mod auth;
mod models;
mod user_state;
mod handlers {
    pub mod auth;
    pub mod calculator;
    pub mod order_processing;
    pub mod product_management;
    pub mod quote_processing;
    pub mod version;
}

use auth::MongoAuth;
use handlers::{
    auth as auth_h, calculator as calc_h, order_processing as op_h, 
    product_management as pm_h, quote_processing as qp_h, version as ver_h,
};
use models::{CustomBadgeQuote, Order, Product, User};

/// Debug function to log directory contents at startup
async fn debug_log_directories() {
    info!("🔍 [DEBUG] Checking directory mounts...");
    
    // Check /static directory (built-in files)
    match tokio::fs::read_dir("/static").await {
        Ok(mut entries) => {
            info!("📋 [DEBUG] Contents of /static directory (built-in):");
            let mut count = 0;
            while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                let name = entry.file_name().to_string_lossy().to_string();
                let file_type = match entry.file_type().await {
                    Ok(ft) if ft.is_dir() => "[DIR]",
                    Ok(_) => "[FILE]",
                    Err(_) => "[UNKNOWN]",
                };
                info!("📋 [DEBUG]   {} {}", file_type, name);
                count += 1;
                if count > 20 { info!("📋 [DEBUG]   ... (truncated, {} items total)", count); break; }
            }
            if count == 0 {
                info!("⚠️ [DEBUG] /static directory is empty!");
            } else {
                info!("✅ [DEBUG] /static directory contains {} items", count);
            }
        }
        Err(e) => {
            info!("❌ [DEBUG] Cannot read /static directory: {}", e);
        }
    }
    
    // Check /product-images directory (NFS mounted)
    match tokio::fs::read_dir("/product-images").await {
        Ok(mut entries) => {
            info!("🖼️ [DEBUG] Contents of /product-images directory (NFS mounted):");
            let mut count = 0;
            while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                let name = entry.file_name().to_string_lossy().to_string();
                let file_type = match entry.file_type().await {
                    Ok(ft) if ft.is_dir() => "[DIR]",
                    Ok(_) => "[FILE]",
                    Err(_) => "[UNKNOWN]",
                };
                info!("🖼️ [DEBUG]   {} {}", file_type, name);
                count += 1;
                if count > 20 { info!("🖼️ [DEBUG]   ... (truncated, {} items total)", count); break; }
            }
            if count == 0 {
                info!("🖼️ [DEBUG] /product-images directory is empty (ready for uploads)");
            } else {
                info!("✅ [DEBUG] /product-images directory contains {} items", count);
            }
        }
        Err(e) => {
            info!("❌ [DEBUG] Cannot read /product-images directory: {} - Check NFS mount!", e);
        }
    }
    
    info!("✅ [DEBUG] Directory check complete");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "foxy_fabrications_admin=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🚀 Starting Foxy Fabrications Admin Server");

    // Load environment variables
    dotenv().ok();

    // MongoDB connection
    let mongodb_uri = env::var("MONGODB_URI").unwrap_or_else(|_| "mongodb://localhost:27017".to_string());
    let client = Client::with_uri_str(&mongodb_uri).await?;
    let db = client.database("foxy_fabrications");

    // Test MongoDB connection
    db.run_command(mongodb::bson::doc! {"ping": 1}).await?;
    info!("✅ MongoDB connected successfully");

    // Ensure product-images directory exists
    if let Err(e) = tokio::fs::create_dir_all("product-images").await {
        info!("⚠️ Failed to create product-images directory: {}. This may cause upload issues.", e);
    } else {
        info!("✅ Product-images directory ready for uploads");
    }
    
    // Debug: Log directory contents for troubleshooting
    debug_log_directories().await;

    // Collections
    let users_coll: Collection<User> = db.collection("users");
    let products_coll: Collection<Product> = db.collection("products");
    let orders_coll: Collection<Order> = db.collection("orders");
    let badge_quotes_coll: Collection<CustomBadgeQuote> = db.collection("badge_quotes");

    // Setup session store and auth
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);
    let auth_backend = MongoAuth {
        users: users_coll.clone(),
    };
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    // Protected admin routes - each handler manages its own auth to allow redirects
    let protected_admin_routes = Router::new()
        // Product Management Routes
        .route("/products", get(pm_h::list_products))
        .route("/products/new", get(pm_h::show_create_form).post(pm_h::create_product))
        .route("/products/edit/{id}", get(pm_h::show_edit_form).post(pm_h::update_product))
        .route("/products/delete/{id}", delete(pm_h::delete_product))
        // Order Processing Routes
        .route("/orders", get(op_h::list_orders))
        .route("/orders/update-status", post(op_h::update_order_status))
        // Quote Processing Routes
        .route("/quotes", get(qp_h::list_quotes))
        .route("/quotes/update-status", post(qp_h::update_quote_status))
        .route("/quotes/image/{filename}", get(qp_h::serve_badge_image))
        // Admin Tools
        .route("/calculator", get(calc_h::show_calculator))
        .layer(Extension(products_coll.clone()))
        .layer(Extension(users_coll.clone()))
        .layer(Extension(orders_coll.clone()))
        .layer(Extension(badge_quotes_coll.clone()))
        .layer(Extension(db.clone()));
    
    // Dashboard route (handles its own auth to redirect properly)
    let dashboard_routes = Router::new()
        .route("/", get(dashboard))
        .layer(Extension(products_coll))
        .layer(Extension(users_coll))
        .layer(Extension(orders_coll))
        .layer(Extension(badge_quotes_coll))
        .layer(Extension(db.clone()));

    // Public routes (login and info/health)
    let public_routes = Router::new()
        .route("/login", get(auth_h::show_login_form).post(auth_h::handle_login))
        .route("/logout", get(auth_h::handle_logout))
        .route("/info", get(ver_h::info))
        .route("/health", get(ver_h::health));

    // Static files and product images
    let static_routes = Router::new()
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/product-images", ServeDir::new("product-images"));

    // Main app
    let app = Router::new()
        .merge(dashboard_routes)
        .merge(protected_admin_routes)
        .merge(public_routes) 
        .merge(static_routes)
        .layer(
            ServiceBuilder::new()
                .layer(CookieManagerLayer::new())
                .layer(auth_layer)
        );

    // Start server on different port (3001) to avoid conflicts
    let port = env::var("ADMIN_PORT").unwrap_or_else(|_| "3001".to_string());
    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port)).await?;
    
    info!("🎯 Foxy Fabrications Admin Server running on http://0.0.0.0:{}", port);
    info!("📊 Admin interface available at: http://localhost:{}", port);
    info!("🔐 Login required - admin users only");
    
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Admin dashboard homepage
async fn dashboard(auth: auth_h::AppAuthSession) -> impl IntoResponse {
    let user_state = user_state::extract_user_state(&auth);
    
    // Redirect unauthenticated users to login
    if !user_state.is_authenticated {
        return Redirect::to("/login").into_response();
    }
    
    // Ensure user is admin
    if !user_state.is_admin {
        return (StatusCode::FORBIDDEN, "Access denied - Admin privileges required").into_response();
    }
    
    // Redirect to products page for now (could be a proper dashboard later)
    Redirect::to("/products").into_response()
}
