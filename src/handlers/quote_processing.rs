use askama::Template;
use axum::{
    Extension,
    extract::{Form, Path, Query},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Json, Response, Redirect},
    body::Body,
};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{doc, oid::ObjectId},
};
use tokio::fs;

use crate::{
    handlers::auth::AppAuthSession,
    models::{
        CustomBadgeQuote, QuoteDisplay, QuoteOperationResponse, QuoteProcessingTemplate, 
        QuoteQueryParams, PaginationInfo, UpdateQuoteStatusForm,
    },
    user_state::extract_user_state,
};

/// Default page size for quote listing
const DEFAULT_PAGE_SIZE: u32 = 10;
const MAX_PAGE_SIZE: u32 = 100;

/// List quotes with pagination and filtering
pub async fn list_quotes(
    Extension(quotes_collection): Extension<Collection<CustomBadgeQuote>>,
    Query(params): Query<QuoteQueryParams>,
    auth: AppAuthSession,
) -> impl IntoResponse {
    let user_state = extract_user_state(&auth);

    // Redirect unauthenticated users to login
    if !user_state.is_authenticated {
        return Redirect::to("/login").into_response();
    }

    // Ensure user is admin
    if !user_state.is_admin {
        return (StatusCode::FORBIDDEN, "Access denied - Admin privileges required").into_response();
    }

    let status_filter = params.status_filter.as_deref().unwrap_or("all");

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE);
    let skip = (page - 1) * page_size;

    // Build filter based on status
    let filter = if status_filter == "all" {
        doc! {} // Show all quotes
    } else {
        doc! { "status": status_filter }
    };

    // Get total count for pagination
    let total_count = match quotes_collection.count_documents(filter.clone()).await {
        Ok(count) => count,
        Err(e) => {
            let template = QuoteProcessingTemplate {
                quotes: vec![],
                pagination: create_pagination_info(1, page_size, 0),
                status_filter: status_filter.to_string(),
                success_message: String::new(),
                error_message: format!("Database error counting quotes: {}", e),
                user_state,
            };
            return Html(template.render().unwrap()).into_response();
        }
    };

    // Fetch quotes with pagination
    let quotes_result = quotes_collection
        .find(filter)
        .skip(skip as u64)
        .limit(page_size as i64)
        .sort(doc! { "created_at": -1 }) // Newest first
        .await;

    let quotes = match quotes_result {
        Ok(cursor) => cursor.try_collect::<Vec<CustomBadgeQuote>>().await.unwrap_or_default(),
        Err(e) => {
            let template = QuoteProcessingTemplate {
                quotes: vec![],
                pagination: create_pagination_info(1, page_size, 0),
                status_filter: status_filter.to_string(),
                success_message: String::new(),
                error_message: format!("Database error fetching quotes: {}", e),
                user_state,
            };
            return Html(template.render().unwrap()).into_response();
        }
    };

    let quote_displays: Vec<QuoteDisplay> = quotes.into_iter().map(convert_to_display).collect();

    let pagination = create_pagination_info(page, page_size, total_count);

    let template = QuoteProcessingTemplate {
        quotes: quote_displays,
        pagination,
        status_filter: status_filter.to_string(),
        success_message: String::new(),
        error_message: String::new(),
        user_state,
    };

    Html(template.render().unwrap()).into_response()
}

/// Update quote status
pub async fn update_quote_status(
    Extension(quotes_collection): Extension<Collection<CustomBadgeQuote>>,
    auth: AppAuthSession,
    Form(form): Form<UpdateQuoteStatusForm>,
) -> impl IntoResponse {
    let user_state = extract_user_state(&auth);

    // Ensure user is admin
    if !user_state.is_admin {
        return Json(QuoteOperationResponse {
            success: false,
            message: "Access denied".to_string(),
            quote_id: None,
        })
        .into_response();
    }

    // Validate status
    let valid_statuses = vec!["pending", "quoted", "accepted", "completed", "cancelled"];
    if !valid_statuses.contains(&form.status.as_str()) {
        return Json(QuoteOperationResponse {
            success: false,
            message: "Invalid status".to_string(),
            quote_id: None,
        })
        .into_response();
    }

    // Parse the hex string into an ObjectID
    let obj_id = match ObjectId::parse_str(&form.quote_id) {
        Ok(oid) => oid,
        Err(_) => {
            return Json(QuoteOperationResponse {
                success: false,
                message: "Invalid quote ID".to_string(),
                quote_id: None,
            })
            .into_response();
        }
    };

    // Update document
    let update_doc = doc! {
        "$set": {
            "status": &form.status,
            "updated_at": Utc::now().to_rfc3339(),
        }
    };

    let filter = doc! { "_id": obj_id };
    match quotes_collection.update_one(filter, update_doc).await {
        Ok(result) => {
            if result.matched_count > 0 {
                Json(QuoteOperationResponse {
                    success: true,
                    message: format!("Quote status updated to {}", form.status),
                    quote_id: Some(form.quote_id.clone()),
                })
                .into_response()
            } else {
                Json(QuoteOperationResponse {
                    success: false,
                    message: "Quote not found".to_string(),
                    quote_id: None,
                })
                .into_response()
            }
        }
        Err(e) => Json(QuoteOperationResponse {
            success: false,
            message: format!("Database error: {}", e),
            quote_id: None,
        })
        .into_response(),
    }
}

/// Serve badge images from private_uploads directory (admin only)
pub async fn serve_badge_image(
    auth: AppAuthSession,
    Path(filename): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    // Ensure user is admin
    if !extract_user_state(&auth).is_admin {
        return Err(StatusCode::FORBIDDEN);
    }

    // Security: Only allow badge files and prevent path traversal
    if !filename.starts_with("badge_") || filename.contains("..") || filename.contains('/') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let filepath = format!("./private_uploads/{}", filename);
    
    match fs::read(&filepath).await {
        Ok(contents) => {
            let content_type = match filepath.rsplit('.').next().unwrap_or("jpg").to_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "gif" => "image/gif",
                _ => "application/octet-stream",
            };
            
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "private, max-age=3600") // Cache for 1 hour
                .body(Body::from(contents))
                .unwrap())
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Convert CustomBadgeQuote to QuoteDisplay for template rendering
fn convert_to_display(quote: CustomBadgeQuote) -> QuoteDisplay {
    // Format the created_at date
    let formatted_created_at = match DateTime::parse_from_rfc3339(&quote.created_at) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => quote.created_at.clone(),
    };

    // Generate status CSS class
    let status_class = match quote.status.as_str() {
        "pending" => "status-pending",
        "quoted" => "status-quoted", 
        "accepted" => "status-accepted",
        "completed" => "status-completed",
        "cancelled" => "status-cancelled",
        _ => "status-unknown",
    };

    // Generate sided text
    let sided_text = if quote.double_sided {
        "Double-sided".to_string()
    } else {
        "Single-sided".to_string()
    };

    QuoteDisplay {
        id: quote.id.to_hex(),
        num_colors: quote.num_colors,
        double_sided: if quote.double_sided { "yes" } else { "no" }.to_string(),
        print_size: quote.print_size,
        thickness: quote.thickness,
        email: quote.email,
        image_path: quote.image_path,
        estimated_price: quote.estimated_price,
        created_at: quote.created_at,
        status: quote.status,
        formatted_price: format!("£{:.2}", quote.estimated_price),
        formatted_created_at,
        status_class: status_class.to_string(),
        sided_text: sided_text.to_string(),
    }
}

/// Create pagination information
fn create_pagination_info(current_page: u32, page_size: u32, total_items: u64) -> PaginationInfo {
    let total_pages = if total_items == 0 {
        1
    } else {
        ((total_items as f64) / (page_size as f64)).ceil() as u32
    };

    let start_item = if total_items == 0 {
        0
    } else {
        ((current_page - 1) * page_size) + 1
    };

    let end_item = if total_items == 0 {
        0
    } else {
        std::cmp::min(current_page * page_size, total_items as u32)
    };

    PaginationInfo {
        current_page,
        total_pages,
        has_prev: current_page > 1,
        has_next: current_page < total_pages,
        start_item,
        end_item,
        total_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::oid::ObjectId;

    #[test]
    fn test_convert_to_display_basic_conversion() {
        let quote = CustomBadgeQuote {
            id: ObjectId::new(),
            num_colors: "3".to_string(),
            double_sided: "no".to_string(),
            print_size: "100mm".to_string(),
            thickness: "10mm".to_string(),
            email: "test@example.com".to_string(),
            image_path: Some("test.jpg".to_string()),
            estimated_price: 25.50,
            created_at: "2025-01-01T12:00:00Z".to_string(),
            status: "pending".to_string(),
        };

        let display = convert_to_display(quote.clone());

        assert_eq!(display.num_colors, "3");
        assert_eq!(display.double_sided, "no");
        assert_eq!(display.print_size, "100mm");
        assert_eq!(display.thickness, "10mm");
        assert_eq!(display.email, "test@example.com");
        assert_eq!(display.estimated_price, 25.50);
        assert_eq!(display.formatted_price, "£25.50");
        assert_eq!(display.status, "pending");
        assert_eq!(display.status_class, "status-pending");
        assert_eq!(display.sided_text, "Single-sided");
        assert_eq!(display.id, quote.id.to_hex());
    }

    #[test]
    fn test_convert_to_display_double_sided() {
        let quote = CustomBadgeQuote {
            id: ObjectId::new(),
            num_colors: "5".to_string(),
            double_sided: "yes".to_string(),
            print_size: "125mm".to_string(),
            thickness: "15mm".to_string(),
            email: "customer@test.com".to_string(),
            image_path: None,
            estimated_price: 42.75,
            created_at: "2025-01-05T15:30:00Z".to_string(),
            status: "quoted".to_string(),
        };

        let display = convert_to_display(quote);

        assert_eq!(display.sided_text, "Double-sided");
        assert_eq!(display.status_class, "status-quoted");
        assert_eq!(display.formatted_price, "£42.75");
    }

    #[test]
    fn test_convert_to_display_all_statuses() {
        let statuses = vec![
            ("pending", "status-pending"),
            ("quoted", "status-quoted"),
            ("accepted", "status-accepted"), 
            ("completed", "status-completed"),
            ("cancelled", "status-cancelled"),
            ("unknown", "status-unknown"),
        ];

        for (status, expected_class) in statuses {
            let quote = CustomBadgeQuote {
                id: ObjectId::new(),
                num_colors: "2".to_string(),
                double_sided: "no".to_string(),
                print_size: "80mm".to_string(),
                thickness: "5mm".to_string(),
                email: "test@example.com".to_string(),
                image_path: None,
                estimated_price: 15.00,
                created_at: "2025-01-01T12:00:00Z".to_string(),
                status: status.to_string(),
            };

            let display = convert_to_display(quote);
            assert_eq!(display.status_class, expected_class);
        }
    }

    #[test]
    fn test_create_pagination_info_basic() {
        let pagination = create_pagination_info(2, 10, 25);
        
        assert_eq!(pagination.current_page, 2);
        assert_eq!(pagination.total_pages, 3);
        assert!(pagination.has_prev);
        assert!(pagination.has_next);
        assert_eq!(pagination.start_item, 11);
        assert_eq!(pagination.end_item, 20);
        assert_eq!(pagination.total_items, 25);
    }

    #[test]
    fn test_create_pagination_info_empty() {
        let pagination = create_pagination_info(1, 10, 0);
        
        assert_eq!(pagination.current_page, 1);
        assert_eq!(pagination.total_pages, 1);
        assert!(!pagination.has_prev);
        assert!(!pagination.has_next);
        assert_eq!(pagination.start_item, 0);
        assert_eq!(pagination.end_item, 0);
        assert_eq!(pagination.total_items, 0);
    }

    #[test]
    fn test_create_pagination_info_last_page() {
        let pagination = create_pagination_info(3, 10, 25);
        
        assert_eq!(pagination.current_page, 3);
        assert_eq!(pagination.total_pages, 3);
        assert!(pagination.has_prev);
        assert!(!pagination.has_next);
        assert_eq!(pagination.start_item, 21);
        assert_eq!(pagination.end_item, 25);
        assert_eq!(pagination.total_items, 25);
    }
}
