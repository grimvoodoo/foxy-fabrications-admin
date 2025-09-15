use askama::Template;
use mongodb::bson::{doc, oid::ObjectId};
use serde::{Deserialize, Serialize};

/// —————————————————————————————
/// User model (Mongo "users" collection)
/// —————————————————————————————
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
}

/// Login form (extract from /login)
#[derive(serde::Deserialize, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

/// User state for templates
#[derive(Debug, Clone, Default)]
pub struct UserState {
    pub is_authenticated: bool,
    pub username: String,
    pub is_admin: bool,
    pub logged_in: bool, // alias for is_authenticated
}

impl UserState {
    pub fn new(is_authenticated: bool, username: String, is_admin: bool) -> Self {
        Self {
            is_authenticated,
            logged_in: is_authenticated, // Keep both for template compatibility
            username,
            is_admin,
        }
    }
}

/// —————————————————————————————
/// Product model (Mongo "products" collection)
/// —————————————————————————————
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub name: String,
    pub image_url: String,
    pub price: String,
    pub quantity: i32,
    pub description: String,
    pub adoptable: bool,
}

/// A display‐safe version for Askama
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductDisplay {
    pub id: String,
    pub name: String,
    pub image_url: String,
    pub price: String,
    pub quantity: i32,
    pub description: String,
    pub adoptable: bool,
}

/// Create product form
#[derive(Deserialize, Serialize)]
pub struct CreateProductForm {
    pub name: String,
    pub price: String,
    pub quantity: String,
    pub description: String,
    pub adoptable: Option<String>,
}

/// —————————————————————————————
/// Order Models
/// —————————————————————————————
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingAddress {
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub postcode: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub product_id: String,  // MongoDB stores this as String, not ObjectId
    #[serde(rename = "name")]
    pub product_name: String, // MongoDB field is "name", we want "product_name" in Rust
    pub quantity: i32,
    pub price: f64,
    pub line_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub order_reference: String,
    pub customer_name: String,
    pub customer_email: String,
    pub shipping_address: ShippingAddress,
    pub items: Vec<OrderItem>,
    pub subtotal: f64,
    pub shipping_cost: f64,
    pub total: f64,
    pub currency: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// —————————————————————————————
/// Custom Badge Quote Models
/// —————————————————————————————
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomBadgeQuote {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub num_colors: String,
    pub double_sided: bool,
    pub print_size: String,
    pub thickness: String,
    pub email: String,
    pub image_path: Option<String>,
    pub estimated_price: f64,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
}

/// —————————————————————————————
/// Product Management Models
/// —————————————————————————————

/// Form for editing existing products
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EditProductForm {
    pub name: String,
    pub price: String,
    pub quantity: String,
    pub description: String,
    pub adoptable: Option<String>, // "on" if checked, None if unchecked
}

/// Template for product management list page
#[derive(Template)]
#[template(path = "product_management.html")]
pub struct ProductManagementTemplate {
    pub products: Vec<ProductDisplay>,
    pub user_state: UserState,
    pub success_message: String,
    pub error_message: String,
}

/// Template for editing a product
#[derive(Template)]
#[template(path = "edit_product.html")]
pub struct EditProductTemplate {
    pub product: ProductDisplay,
    pub user_state: UserState,
    pub error_message: String,
}

/// Response for product operations (JSON)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProductOperationResponse {
    pub success: bool,
    pub message: String,
    pub product_id: Option<String>,
}

/// —————————————————————————————
/// Order Processing Models
/// —————————————————————————————

/// Display version of ShippingAddress for templates (all strings, no Options)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingAddressDisplay {
    pub line1: String,
    pub line2: String, // Empty string if None
    pub city: String,
    pub postcode: String,
    pub country: String,
}

/// Display version of Order for templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDisplay {
    pub id: String,
    pub order_reference: String,
    pub customer_name: String,
    pub customer_email: String,
    pub shipping_address: ShippingAddressDisplay,
    pub items: Vec<OrderItem>,
    pub subtotal: f64,
    pub shipping_cost: f64,
    pub total: f64,
    pub currency: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub formatted_total: String,
    pub formatted_created_at: String,
    pub status_class: String, // CSS class for status badge
}

/// Query parameters for order processing page
#[derive(Deserialize, Debug, Clone)]
pub struct OrderQueryParams {
    pub page: Option<u32>,
    pub show_completed: Option<String>, // "true" or "false"
    pub page_size: Option<u32>,
}

/// Pagination information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub current_page: u32,
    pub total_pages: u32,
    pub has_prev: bool,
    pub has_next: bool,
    pub start_item: u32,
    pub end_item: u32,
    pub total_items: u64,
}

/// Template for order processing page
#[derive(Template)]
#[template(path = "order_processing.html")]
pub struct OrderProcessingTemplate {
    pub orders: Vec<OrderDisplay>,
    pub pagination: PaginationInfo,
    pub show_completed: bool,
    pub success_message: String,
    pub error_message: String,
    pub user_state: UserState,
}

/// Form for updating order status
#[derive(Deserialize, Debug, Clone)]
pub struct UpdateOrderStatusForm {
    pub order_id: String,
    pub status: String,
}

/// Response for order operations (JSON)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OrderOperationResponse {
    pub success: bool,
    pub message: String,
    pub order_id: Option<String>,
}

/// —————————————————————————————
/// Quote Processing Models
/// —————————————————————————————

/// Display version of CustomBadgeQuote for templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteDisplay {
    pub id: String,
    pub num_colors: String,
    pub double_sided: String,
    pub print_size: String,
    pub thickness: String,
    pub email: String,
    pub image_path: Option<String>,
    pub estimated_price: f64,
    pub created_at: String,
    pub status: String,
    pub formatted_price: String,
    pub formatted_created_at: String,
    pub status_class: String, // CSS class for status badge
    pub sided_text: String,   // "Single-sided" or "Double-sided"
}

/// Query parameters for quote processing page
#[derive(Deserialize, Debug, Clone)]
pub struct QuoteQueryParams {
    pub page: Option<u32>,
    pub status_filter: Option<String>, // "pending", "quoted", "accepted", "completed" or "all"
    pub page_size: Option<u32>,
}

/// Template for quote processing page
#[derive(Template)]
#[template(path = "quote_processing.html")]
pub struct QuoteProcessingTemplate {
    pub quotes: Vec<QuoteDisplay>,
    pub pagination: PaginationInfo,
    pub status_filter: String,
    pub success_message: String,
    pub error_message: String,
    pub user_state: UserState,
}

/// Form for updating quote status
#[derive(Deserialize, Debug, Clone)]
pub struct UpdateQuoteStatusForm {
    pub quote_id: String,
    pub status: String,
}

/// Response for quote operations (JSON)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuoteOperationResponse {
    pub success: bool,
    pub message: String,
    pub quote_id: Option<String>,
}

/// —————————————————————————————
/// Calculator Template
/// —————————————————————————————
#[derive(Template)]
#[template(path = "calculator.html")]
pub struct CalculatorTemplate {
    pub user_state: UserState,
}

/// —————————————————————————————
/// Auth Templates
/// —————————————————————————————
#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub next: String,
    pub next_url: String, // alias for next
    pub error: String,
    pub user_state: UserState,
}
