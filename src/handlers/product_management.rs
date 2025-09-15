use askama::Template;
use axum::{
    Extension,
    extract::{Form, Path},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
};
use futures_util::TryStreamExt;
use mongodb::{
    Collection,
    bson::{doc, oid::ObjectId},
};

use crate::{
    handlers::auth::AppAuthSession,
    models::{
        EditProductForm, EditProductTemplate, Product, ProductDisplay, ProductManagementTemplate,
        ProductOperationResponse, UserState,
    },
    user_state::extract_user_state,
};

/// Normalize image URL to ensure it starts with /
fn normalize_image_url(url: &str) -> String {
    if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{}", url)
    }
}

/// List all products for admin management
pub async fn list_products(
    Extension(collection): Extension<Collection<Product>>,
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

    // Get all products (including out of stock and adoptables)
    match collection.find(doc! {}).await {
        Ok(cursor) => {
            let products: Vec<ProductDisplay> = cursor
                .try_collect::<Vec<Product>>()
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|p| ProductDisplay {
                    id: p.id.to_hex(),
                    name: p.name,
                    image_url: normalize_image_url(&p.image_url),
                    price: p.price,
                    quantity: p.quantity,
                    description: p.description,
                    adoptable: p.adoptable,
                })
                .collect();

            let template = ProductManagementTemplate {
                products,
                user_state,
                success_message: String::new(),
                error_message: String::new(),
            };

            Html(template.render().unwrap()).into_response()
        }
        Err(e) => {
            let template = ProductManagementTemplate {
                products: vec![],
                user_state,
                success_message: String::new(),
                error_message: format!("Database error: {}", e),
            };

            Html(template.render().unwrap()).into_response()
        }
    }
}

/// Show edit form for a specific product
pub async fn show_edit_form(
    Path(id): Path<String>,
    Extension(collection): Extension<Collection<Product>>,
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

    // Parse the hex string into an ObjectID
    let obj_id = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid product ID").into_response();
        }
    };

    // Find the product
    match collection.find_one(doc! { "_id": obj_id }).await {
        Ok(Some(product)) => {
            let product_display = ProductDisplay {
                id: product.id.to_hex(),
                name: product.name,
                image_url: normalize_image_url(&product.image_url),
                price: product.price,
                quantity: product.quantity,
                description: product.description,
                adoptable: product.adoptable,
            };

            let template = EditProductTemplate {
                product: product_display,
                user_state,
                error_message: String::new(),
            };

            Html(template.render().unwrap()).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Product not found").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
            .into_response(),
    }
}

/// Handle product update
pub async fn update_product(
    Path(id): Path<String>,
    Extension(collection): Extension<Collection<Product>>,
    auth: AppAuthSession,
    Form(form): Form<EditProductForm>,
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

    // Parse the hex string into an ObjectID
    let obj_id = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid product ID").into_response();
        }
    };

    // Validate form data
    let validation_result = validate_product_form(&form);
    if let Err(error_msg) = validation_result {
        // Return to edit form with error
        return show_edit_form_with_error(obj_id, &collection, user_state, error_msg).await;
    }

    let (_validated_price, validated_quantity) = validation_result.unwrap();

    // Update the product in database
    let update_doc = doc! {
        "$set": {
            "name": &form.name,
            "price": &form.price,
            "quantity": validated_quantity,
            "description": &form.description,
            "adoptable": form.adoptable.is_some(),
        }
    };

    match collection
        .update_one(doc! { "_id": obj_id }, update_doc)
        .await
    {
        Ok(result) => {
            if result.matched_count == 0 {
                (StatusCode::NOT_FOUND, "Product not found").into_response()
            } else {
                Redirect::to("/products?success=updated").into_response()
            }
        }
        Err(e) => {
            show_edit_form_with_error(
                obj_id,
                &collection,
                user_state,
                format!("Database error: {}", e),
            )
            .await
        }
    }
}

/// Delete a product
pub async fn delete_product(
    Path(id): Path<String>,
    Extension(collection): Extension<Collection<Product>>,
    auth: AppAuthSession,
) -> impl IntoResponse {
    let user_state = extract_user_state(&auth);

    // Ensure user is admin
    if !user_state.is_admin {
        return Json(ProductOperationResponse {
            success: false,
            message: "Access denied".to_string(),
            product_id: None,
        })
        .into_response();
    }

    // Parse the hex string into an ObjectID
    let obj_id = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => {
            return Json(ProductOperationResponse {
                success: false,
                message: "Invalid product ID".to_string(),
                product_id: None,
            })
            .into_response();
        }
    };

    // Delete the product
    match collection.delete_one(doc! { "_id": obj_id }).await {
        Ok(result) => {
            if result.deleted_count == 0 {
                Json(ProductOperationResponse {
                    success: false,
                    message: "Product not found".to_string(),
                    product_id: None,
                })
                .into_response()
            } else {
                Json(ProductOperationResponse {
                    success: true,
                    message: "Product deleted successfully".to_string(),
                    product_id: Some(id),
                })
                .into_response()
            }
        }
        Err(e) => Json(ProductOperationResponse {
            success: false,
            message: format!("Database error: {}", e),
            product_id: None,
        })
        .into_response(),
    }
}

/// Helper function to validate product form data
pub fn validate_product_form(form: &EditProductForm) -> Result<(f64, i32), String> {
    // Validate name
    if form.name.trim().is_empty() {
        return Err("Product name cannot be empty".to_string());
    }

    if form.name.len() > 255 {
        return Err("Product name must be less than 255 characters".to_string());
    }

    // Validate price
    let price = form
        .price
        .trim()
        .parse::<f64>()
        .map_err(|_| "Price must be a valid number".to_string())?;

    if price < 0.0 {
        return Err("Price cannot be negative".to_string());
    }

    if price > 999999.99 {
        return Err("Price cannot exceed £999,999.99".to_string());
    }

    // Validate quantity
    let quantity = form
        .quantity
        .trim()
        .parse::<i32>()
        .map_err(|_| "Quantity must be a valid number".to_string())?;

    if quantity < 0 {
        return Err("Quantity cannot be negative".to_string());
    }

    if quantity > 999999 {
        return Err("Quantity cannot exceed 999,999".to_string());
    }

    // Validate description
    if form.description.len() > 5000 {
        return Err("Description must be less than 5,000 characters".to_string());
    }

    Ok((price, quantity))
}

/// Helper function to show edit form with error message
async fn show_edit_form_with_error(
    obj_id: ObjectId,
    collection: &Collection<Product>,
    user_state: UserState,
    error_message: String,
) -> axum::response::Response {
    match collection.find_one(doc! { "_id": obj_id }).await {
        Ok(Some(product)) => {
            let product_display = ProductDisplay {
                id: product.id.to_hex(),
                name: product.name,
                image_url: product.image_url,
                price: product.price,
                quantity: product.quantity,
                description: product.description,
                adoptable: product.adoptable,
            };

            let template = EditProductTemplate {
                product: product_display,
                user_state,
                error_message,
            };

            Html(template.render().unwrap()).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Product not found").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database error: {}", e),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_product_form_valid() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "19.99".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_ok());
        let (price, quantity) = result.unwrap();
        assert_eq!(price, 19.99);
        assert_eq!(quantity, 10);
    }

    #[test]
    fn test_validate_product_form_empty_name() {
        let form = EditProductForm {
            name: "".to_string(),
            price: "19.99".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Product name cannot be empty");
    }

    #[test]
    fn test_validate_product_form_invalid_price() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "invalid".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Price must be a valid number");
    }

    #[test]
    fn test_validate_product_form_negative_price() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "-10.99".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Price cannot be negative");
    }

    #[test]
    fn test_validate_product_form_invalid_quantity() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "19.99".to_string(),
            quantity: "invalid".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quantity must be a valid number");
    }

    #[test]
    fn test_validate_product_form_negative_quantity() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "19.99".to_string(),
            quantity: "-5".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quantity cannot be negative");
    }

    #[test]
    fn test_validate_product_form_long_name() {
        let form = EditProductForm {
            name: "a".repeat(256),
            price: "19.99".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Product name must be less than 255 characters"
        );
    }

    #[test]
    fn test_validate_product_form_long_description() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "19.99".to_string(),
            quantity: "10".to_string(),
            description: "a".repeat(5001),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Description must be less than 5,000 characters"
        );
    }

    #[test]
    fn test_validate_product_form_max_price() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "1000000.00".to_string(),
            quantity: "10".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Price cannot exceed £999,999.99");
    }

    #[test]
    fn test_validate_product_form_max_quantity() {
        let form = EditProductForm {
            name: "Test Product".to_string(),
            price: "19.99".to_string(),
            quantity: "1000000".to_string(),
            description: "A great product".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Quantity cannot exceed 999,999");
    }

    #[test]
    fn test_validate_product_form_adoptable_set() {
        let form = EditProductForm {
            name: "Adoptable Pet".to_string(),
            price: "0.00".to_string(),
            quantity: "1".to_string(),
            description: "A cute pet".to_string(),
            adoptable: Some("on".to_string()),
        };

        let result = validate_product_form(&form);
        assert!(result.is_ok());
        let (price, quantity) = result.unwrap();
        assert_eq!(price, 0.0);
        assert_eq!(quantity, 1);
    }

    #[test]
    fn test_validate_product_form_zero_values() {
        let form = EditProductForm {
            name: "Free Item".to_string(),
            price: "0.00".to_string(),
            quantity: "0".to_string(),
            description: "".to_string(),
            adoptable: None,
        };

        let result = validate_product_form(&form);
        assert!(result.is_ok());
        let (price, quantity) = result.unwrap();
        assert_eq!(price, 0.0);
        assert_eq!(quantity, 0);
    }
}
