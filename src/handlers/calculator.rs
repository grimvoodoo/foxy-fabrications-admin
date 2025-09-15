use askama::Template;
use axum::response::Html;

use crate::{handlers::auth::AppAuthSession, models::UserState, user_state::extract_user_state};

#[derive(Template)]
#[template(path = "calculator.html")]
struct CalculatorTemplate {
    user_state: UserState,
}

pub async fn show_calculator(auth: AppAuthSession) -> Html<String> {
    let user_state = extract_user_state(&auth);
    Html(CalculatorTemplate { user_state }.render().unwrap())
}
