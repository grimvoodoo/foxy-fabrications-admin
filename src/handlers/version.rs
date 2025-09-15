use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use std::env;
use tokio::fs;
use tracing::warn;

#[derive(Serialize)]
pub struct VersionInfo {
    pub service: String,
    pub image: String,
    pub build_time: String,
    pub git_commit: String,
    pub environment: String,
}

/// Get version and deployment information about the running service
pub async fn info() -> impl IntoResponse {
    // Try to read version info from file (updated by CI/CD)
    let version_content = match fs::read_to_string("version.txt").await {
        Ok(content) => content.trim().to_string(),
        Err(_) => "unknown".to_string(),
    };

    // Parse version.txt format: "image_name:tag,build_time,git_commit"
    let (image, build_time, git_commit) = if version_content.contains(',') {
        let parts: Vec<&str> = version_content.splitn(3, ',').collect();
        (
            parts.get(0).unwrap_or(&"unknown").to_string(),
            parts.get(1).unwrap_or(&"unknown").to_string(),
            parts.get(2).unwrap_or(&"unknown").to_string(),
        )
    } else {
        // Fallback: treat entire content as image name
        (version_content, "unknown".to_string(), "unknown".to_string())
    };

    let version_info = VersionInfo {
        service: "foxy-fabrications-admin".to_string(),
        image,
        build_time,
        git_commit,
        environment: env::var("ENVIRONMENT").unwrap_or_else(|_| "unknown".to_string()),
    };

    (StatusCode::OK, Json(version_info))
}

/// Health check endpoint
pub async fn health() -> impl IntoResponse {
    #[derive(Serialize)]
    struct HealthStatus {
        status: String,
        service: String,
    }

    let health = HealthStatus {
        status: "healthy".to_string(),
        service: "foxy-fabrications-admin".to_string(),
    };

    (StatusCode::OK, Json(health))
}