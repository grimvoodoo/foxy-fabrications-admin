use askama::Template;
use axum::{
    Extension,
    extract::{Form, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
};
use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{doc, oid::ObjectId},
};
use tracing::{info, warn, error};

use crate::{
    handlers::auth::AppAuthSession,
    models::{
        Order, OrderDisplay, OrderOperationResponse, OrderProcessingTemplate, OrderQueryParams,
        PaginationInfo, ShippingAddressDisplay, UpdateOrderStatusForm,
    },
    user_state::extract_user_state,
};

/// Default page size for order listing
const DEFAULT_PAGE_SIZE: u32 = 10;
const MAX_PAGE_SIZE: u32 = 100;

/// List orders with pagination and filtering
pub async fn list_orders(
    Extension(orders_collection): Extension<Collection<Order>>,
    Extension(database): Extension<mongodb::Database>,
    Query(params): Query<OrderQueryParams>,
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

    let show_completed = params.show_completed.as_deref().unwrap_or("false") == "true";

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE);
    let skip = (page - 1) * page_size;

    // Get references to both collections
    let completed_orders_collection = database.collection::<Order>("completed_orders");
    
    info!("Connecting to database: {}, main orders collection: orders, completed collection: completed_orders", database.name());

    // Fetch orders from both collections
    let mut all_orders = Vec::new();

    // Always fetch from the main orders collection (unpaid orders: pending, failed, cancelled)
    let main_filter = if show_completed {
        doc! {} // Show all orders from main collection when showing completed
    } else {
        doc! { "status": { "$in": ["pending", "failed", "cancelled"] } }
    };

    info!("Querying main orders collection with filter: {:?}", main_filter);
    match orders_collection.find(main_filter.clone()).await {
        Ok(cursor) => {
            // First, let's try to get raw documents using the Database collection
            let raw_collection = database.collection::<mongodb::bson::Document>("orders");
            let raw_docs: Result<Vec<mongodb::bson::Document>, _> = 
                raw_collection.find(main_filter.clone()).await
                    .unwrap().try_collect().await;
            
            match raw_docs {
                Ok(docs) => {
                    info!("Found {} raw documents in main orders collection", docs.len());
                    for (i, doc) in docs.iter().take(2).enumerate() {
                        info!("Raw doc {}: {:?}", i, doc);
                    }
                },
                Err(e) => error!("Error getting raw documents: {:?}", e)
            }
            
            // Now try to deserialize to Order structs
            let orders: Vec<Order> = cursor.try_collect().await.unwrap_or_default();
            info!("Successfully deserialized {} orders from main collection", orders.len());
            for order in &orders {
                info!("Order: {} - Status: {}", order.order_reference, order.status);
            }
            all_orders.extend(orders);
        }
        Err(e) => {
            let template = OrderProcessingTemplate {
                orders: vec![],
                pagination: create_pagination_info(1, page_size, 0),
                show_completed,
                success_message: String::new(),
                error_message: format!("Database error fetching orders: {}", e),
                user_state,
            };
            return Html(template.render().unwrap()).into_response();
        }
    }

    // Fetch from completed orders collection (orders that have been paid and need processing)
    let completed_filter = if show_completed {
        doc! {} // Show all orders from completed_orders collection
    } else {
        doc! { "status": { "$in": ["paid", "processing", "shipped"] } } // Show orders that need attention
    };

    info!("Querying completed_orders collection with filter: {:?}", completed_filter);
    match completed_orders_collection.find(completed_filter.clone()).await {
        Ok(cursor) => {
            // Check raw documents first using Database collection
            let raw_completed_collection = database.collection::<mongodb::bson::Document>("completed_orders");
            let raw_docs: Result<Vec<mongodb::bson::Document>, _> = 
                raw_completed_collection.find(completed_filter.clone()).await
                    .unwrap().try_collect().await;
            
            match raw_docs {
                Ok(docs) => {
                    info!("Found {} raw documents in completed_orders collection", docs.len());
                    for (i, doc) in docs.iter().take(2).enumerate() {
                        info!("Raw completed doc {}: {:?}", i, doc);
                    }
                },
                Err(e) => error!("Error getting raw completed documents: {:?}", e)
            }
            
            let completed_orders: Vec<Order> = cursor.try_collect().await.unwrap_or_default();
            info!("Successfully deserialized {} orders from completed_orders collection", completed_orders.len());
            for order in &completed_orders {
                info!("Completed Order: {} - Status: {}", order.order_reference, order.status);
            }
            all_orders.extend(completed_orders);
        }
        Err(_) => {
            // It's ok if completed_orders collection doesn't exist yet - just continue
        }
    }

    // Sort all orders by created_at (newest first)
    all_orders.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    
    info!("Total combined orders: {}, show_completed: {}", all_orders.len(), show_completed);
    
let total_count = all_orders.len() as u64;
    
    // Apply pagination manually since we're combining collections
    let skip_usize = skip as usize;
    let page_size_usize = page_size as usize;
    let paginated_orders: Vec<Order> = all_orders
        .into_iter()
        .skip(skip_usize)
        .take(page_size_usize)
        .collect();

    let order_displays: Vec<OrderDisplay> = paginated_orders.into_iter().map(convert_to_display).collect();

    let pagination = create_pagination_info(page, page_size, total_count);

    let template = OrderProcessingTemplate {
        orders: order_displays,
        pagination,
        show_completed,
        success_message: String::new(),
        error_message: String::new(),
        user_state,
    };

    Html(template.render().unwrap()).into_response()
}

/// Update order status
pub async fn update_order_status(
    Extension(orders_collection): Extension<Collection<Order>>,
    Extension(database): Extension<mongodb::Database>,
    auth: AppAuthSession,
    Form(form): Form<UpdateOrderStatusForm>,
) -> impl IntoResponse {
    let user_state = extract_user_state(&auth);

    // Ensure user is admin
    if !user_state.is_admin {
        return Json(OrderOperationResponse {
            success: false,
            message: "Access denied".to_string(),
            order_id: None,
        })
        .into_response();
    }

    // Validate status
    let valid_statuses = vec!["paid", "processing", "shipped", "completed", "cancelled"];
    if !valid_statuses.contains(&form.status.as_str()) {
        return Json(OrderOperationResponse {
            success: false,
            message: "Invalid status".to_string(),
            order_id: None,
        })
        .into_response();
    }

    // Parse the hex string into an ObjectID
    let obj_id = match ObjectId::parse_str(&form.order_id) {
        Ok(oid) => oid,
        Err(_) => {
            return Json(OrderOperationResponse {
                success: false,
                message: "Invalid order ID".to_string(),
                order_id: None,
            })
            .into_response();
        }
    };

    // Get reference to completed orders collection
    let completed_orders_collection = database.collection::<Order>("completed_orders");
    
    // Update document
    let update_doc = doc! {
        "$set": {
            "status": &form.status,
            "updated_at": Utc::now().to_rfc3339(),
        }
    };

    // Try to update in main orders collection first
    let filter = doc! { "_id": obj_id };
    match orders_collection.update_one(filter.clone(), update_doc.clone()).await {
        Ok(result) => {
            if result.matched_count > 0 {
                // Successfully updated in main collection
                Json(OrderOperationResponse {
                    success: true,
                    message: format!("Order status updated to {}", form.status),
                    order_id: Some(form.order_id.clone()),
                })
                .into_response()
            } else {
                // Not found in main collection, try completed orders collection
                match completed_orders_collection.update_one(filter, update_doc).await {
                    Ok(result) => {
                        if result.matched_count > 0 {
                            Json(OrderOperationResponse {
                                success: true,
                                message: format!("Order status updated to {}", form.status),
                                order_id: Some(form.order_id.clone()),
                            })
                            .into_response()
                        } else {
                            Json(OrderOperationResponse {
                                success: false,
                                message: "Order not found in either collection".to_string(),
                                order_id: None,
                            })
                            .into_response()
                        }
                    }
                    Err(e) => Json(OrderOperationResponse {
                        success: false,
                        message: format!("Database error updating completed order: {}", e),
                        order_id: None,
                    })
                    .into_response(),
                }
            }
        }
        Err(e) => Json(OrderOperationResponse {
            success: false,
            message: format!("Database error: {}", e),
            order_id: None,
        })
        .into_response(),
    }
}

/// Convert Order to OrderDisplay for template rendering
pub fn convert_to_display(order: Order) -> OrderDisplay {
    let status_class = match order.status.as_str() {
        "paid" => "status-paid",
        "processing" => "status-processing",
        "shipped" => "status-shipped",
        "completed" => "status-completed",
        "cancelled" => "status-cancelled",
        _ => "status-unknown",
    };

    let formatted_created_at = match DateTime::parse_from_rfc3339(&order.created_at) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => order.created_at.clone(),
    };

    OrderDisplay {
        id: order.id.to_hex(),
        order_reference: order.order_reference,
        customer_name: order.customer_name,
        customer_email: order.customer_email,
        shipping_address: ShippingAddressDisplay {
            line1: order.shipping_address.line1,
            line2: order.shipping_address.line2.unwrap_or_default(),
            city: order.shipping_address.city,
            postcode: order.shipping_address.postcode,
            country: order.shipping_address.country,
        },
        items: order.items,
        subtotal: order.subtotal,
        shipping_cost: order.shipping_cost,
        total: order.total,
        currency: order.currency,
        status: order.status,
        created_at: order.created_at,
        updated_at: order.updated_at,
        formatted_total: format!("£{:.2}", order.total),
        formatted_created_at,
        status_class: status_class.to_string(),
    }
}

/// Create pagination information
pub fn create_pagination_info(current_page: u32, page_size: u32, total_items: u64) -> PaginationInfo {
    let total_pages = if total_items == 0 {
        1
    } else {
        ((total_items as f64) / (page_size as f64)).ceil() as u32
    };

    let has_prev = current_page > 1;
    let has_next = current_page < total_pages;

    let (start_item, end_item) = if total_items == 0 {
        (0, 0)
    } else {
        let start = (current_page - 1) * page_size + 1;
        let end = (start + page_size - 1).min(total_items as u32);
        (start, end)
    };

    PaginationInfo {
        current_page,
        total_pages,
        has_prev,
        has_next,
        start_item,
        end_item,
        total_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::oid::ObjectId;
    use crate::models::{Order, ShippingAddress, OrderItem};

    fn create_test_order() -> Order {
        Order {
            id: ObjectId::new(),
            order_reference: "ORD-12345".to_string(),
            customer_name: "John Doe".to_string(),
            customer_email: "john@example.com".to_string(),
            shipping_address: ShippingAddress {
                line1: "123 Main St".to_string(),
                line2: Some("Apt 4B".to_string()),
                city: "Springfield".to_string(),
                postcode: "12345".to_string(),
                country: "US".to_string(),
            },
            items: vec![OrderItem {
                product_id: ObjectId::new().to_hex(),
                name: "Test Product".to_string(),
                price: 19.99,
                quantity: 2,
                line_total: 39.98,
            }],
            subtotal: 39.98,
            shipping_cost: 5.00,
            total: 44.98,
            currency: "GBP".to_string(),
            sumup_checkout_id: Some("checkout_123".to_string()),
            sumup_transaction_id: None,
            status: "paid".to_string(),
            created_at: "2025-01-01T12:00:00Z".to_string(),
            updated_at: "2025-01-01T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_convert_to_display_basic_conversion() {
        let order = create_test_order();
        let display = convert_to_display(order.clone());
        
        assert_eq!(display.id, order.id.to_hex());
        assert_eq!(display.customer_name, order.customer_name);
        assert_eq!(display.customer_email, order.customer_email);
        assert_eq!(display.total, order.total);
        assert_eq!(display.currency, order.currency);
        assert_eq!(display.status, order.status);
        assert_eq!(display.formatted_total, "£44.98");
    }

    #[test]
    fn test_convert_to_display_shipping_address_with_line2() {
        let order = create_test_order();
        let display = convert_to_display(order);
        
        assert_eq!(display.shipping_address.line1, "123 Main St");
        assert_eq!(display.shipping_address.line2, "Apt 4B");
        assert_eq!(display.shipping_address.city, "Springfield");
        assert_eq!(display.shipping_address.postcode, "12345");
        assert_eq!(display.shipping_address.country, "US");
    }

    #[test]
    fn test_convert_to_display_shipping_address_without_line2() {
        let mut order = create_test_order();
        order.shipping_address.line2 = None;
        
        let display = convert_to_display(order);
        
        assert_eq!(display.shipping_address.line2, "");
    }

    #[test]
    fn test_convert_to_display_status_classes() {
        let test_cases = vec![
            ("paid", "status-paid"),
            ("processing", "status-processing"),
            ("shipped", "status-shipped"),
            ("completed", "status-completed"),
            ("cancelled", "status-cancelled"),
            ("unknown_status", "status-unknown"),
        ];
        
        for (status, expected_class) in test_cases {
            let mut order = create_test_order();
            order.status = status.to_string();
            
            let display = convert_to_display(order);
            
            assert_eq!(display.status_class, expected_class, "Failed for status: {}", status);
        }
    }

    #[test]
    fn test_convert_to_display_date_formatting() {
        let order = create_test_order();
        let display = convert_to_display(order);
        
        assert_eq!(display.formatted_created_at, "2025-01-01 12:00");
    }

    #[test]
    fn test_convert_to_display_invalid_date() {
        let mut order = create_test_order();
        order.created_at = "invalid-date".to_string();
        
        let display = convert_to_display(order);
        
        assert_eq!(display.formatted_created_at, "invalid-date");
    }

    #[test]
    fn test_create_pagination_info_basic() {
        let pagination = create_pagination_info(2, 10, 50);
        
        assert_eq!(pagination.current_page, 2);
        assert_eq!(pagination.total_pages, 5);
        assert_eq!(pagination.total_items, 50);
        assert!(pagination.has_prev);
        assert!(pagination.has_next);
        assert_eq!(pagination.start_item, 11);
        assert_eq!(pagination.end_item, 20);
    }

    #[test]
    fn test_create_pagination_info_first_page() {
        let pagination = create_pagination_info(1, 10, 50);
        
        assert_eq!(pagination.current_page, 1);
        assert!(!pagination.has_prev);
        assert!(pagination.has_next);
        assert_eq!(pagination.start_item, 1);
        assert_eq!(pagination.end_item, 10);
    }

    #[test]
    fn test_create_pagination_info_last_page() {
        let pagination = create_pagination_info(5, 10, 50);
        
        assert_eq!(pagination.current_page, 5);
        assert!(pagination.has_prev);
        assert!(!pagination.has_next);
        assert_eq!(pagination.start_item, 41);
        assert_eq!(pagination.end_item, 50);
    }

    #[test]
    fn test_create_pagination_info_partial_last_page() {
        let pagination = create_pagination_info(3, 10, 25);
        
        assert_eq!(pagination.current_page, 3);
        assert_eq!(pagination.total_pages, 3);
        assert!(pagination.has_prev);
        assert!(!pagination.has_next);
        assert_eq!(pagination.start_item, 21);
        assert_eq!(pagination.end_item, 25);
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
    }

    #[test]
    fn test_create_pagination_info_single_page() {
        let pagination = create_pagination_info(1, 10, 5);
        
        assert_eq!(pagination.current_page, 1);
        assert_eq!(pagination.total_pages, 1);
        assert!(!pagination.has_prev);
        assert!(!pagination.has_next);
        assert_eq!(pagination.start_item, 1);
        assert_eq!(pagination.end_item, 5);
    }

    #[test]
    fn test_create_pagination_info_edge_cases() {
        // Large page size
        let pagination = create_pagination_info(1, 100, 50);
        assert_eq!(pagination.total_pages, 1);
        assert_eq!(pagination.start_item, 1);
        assert_eq!(pagination.end_item, 50);
        
        // Exact division
        let pagination = create_pagination_info(2, 5, 10);
        assert_eq!(pagination.total_pages, 2);
        assert_eq!(pagination.current_page, 2);
        assert!(pagination.has_prev);
        assert!(!pagination.has_next);
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_PAGE_SIZE, 10);
        assert_eq!(MAX_PAGE_SIZE, 100);
    }
}
