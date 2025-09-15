use askama::Template;
use axum::{
    Form,
    extract::Query,
    response::{Html, IntoResponse, Redirect},
};
use axum_login::AuthSession;
use serde::Deserialize;

use crate::{
    auth::MongoAuth,
    models::{Credentials, LoginTemplate},
    user_state::extract_user_state,
};

// —————————————————————————————
// Redirect parameter handling
// —————————————————————————————
#[derive(Deserialize)]
pub struct RedirectQuery {
    next: Option<String>,
}

fn validate_redirect_url(url: &str) -> bool {
    // Only allow relative URLs for security
    url.starts_with('/') && !url.starts_with("//")
}

fn get_safe_redirect_url(query: Option<String>) -> String {
    match query {
        Some(url) if validate_redirect_url(&url) => url,
        _ => "/products".to_string(), // Default redirect
    }
}


// —————————————————————————————
// Login
// —————————————————————————————

pub async fn show_login_form(
    auth: AppAuthSession,
    Query(query): Query<RedirectQuery>,
) -> impl IntoResponse {
    let user_state = extract_user_state(&auth);
    let next_url = query.next.unwrap_or_default();
    Html(
        LoginTemplate {
            error: "".into(),
            next_url,
            user_state,
        }
        .render()
        .unwrap(),
    )
}

pub type AppAuthSession = AuthSession<MongoAuth>;

#[axum::debug_handler]
pub async fn handle_login(
    mut auth: AppAuthSession,
    Form(creds): Form<Credentials>,
) -> impl IntoResponse {
    let next_url = creds.next.clone().unwrap_or_default();

    match auth.authenticate(creds.clone()).await {
        Ok(Some(u)) => {
            if auth.login(&u).await.is_err() {
                let user_state = extract_user_state(&auth);
                let tpl = LoginTemplate {
                    error: "Internal error".into(),
                    next_url,
                    user_state,
                };
                Html(tpl.render().unwrap()).into_response()
            } else {
                let redirect_url = get_safe_redirect_url(creds.next);
                Redirect::to(&redirect_url).into_response()
            }
        }
        Ok(None) => {
            let user_state = extract_user_state(&auth);
            Html(
                LoginTemplate {
                    error: "Bad credentials".into(),
                    next_url,
                    user_state,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
        Err(_) => {
            let user_state = extract_user_state(&auth);
            Html(
                LoginTemplate {
                    error: "Server error".into(),
                    next_url,
                    user_state,
                }
                .render()
                .unwrap(),
            )
            .into_response()
        }
    }
}

// —————————————————————————————
// Logout
// —————————————————————————————
pub async fn handle_logout(mut auth: AppAuthSession) -> impl IntoResponse {
    if auth.logout().await.is_err() {
        return Redirect::to("/").into_response();
    }
    Redirect::to("/").into_response()
}
