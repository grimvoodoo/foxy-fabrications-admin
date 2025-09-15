use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use std::{collections::HashSet, future::Future, pin::Pin};
use thiserror::Error;

use axum_login::{AuthUser, AuthnBackend, AuthzBackend, UserId};
use mongodb::bson::doc;
use mongodb::{Collection, bson::oid::ObjectId};

use crate::models::{Credentials, User};

/// A sized, concrete error type you can `#[from]`.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("database error: {0}")]
    Database(#[from] mongodb::error::Error),

    #[error("internal error: {0}")]
    Other(String),
}

impl AuthUser for User {
    type Id = ObjectId;
    fn id(&self) -> Self::Id {
        self.id.clone()
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

/// Argon2‐based password hashing & verifying
pub async fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(hash) => hash.to_string(),
        Err(_) => {
            // Fallback in case of hashing error (extremely rare)
            // Use a simpler salt generation
            let fallback_salt = SaltString::generate(&mut OsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), &fallback_salt)
                .map(|h| h.to_string())
                .unwrap_or_else(|_| "$argon2id$error$hash".to_string())
        }
    }
}

pub async fn verify_password(submitted: &str, stored_hash: &str) -> bool {
    let parsed = match PasswordHash::new(stored_hash) {
        Ok(phc) => phc,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(submitted.as_bytes(), &parsed)
        .is_ok()
}

/// Authentication logic helpers (testable synchronous functions)
pub mod auth_helpers {
    use super::*;

    /// Validate credentials format (synchronous helper)
    pub fn validate_credentials(creds: &Credentials) -> Result<(), String> {
        if creds.username.trim().is_empty() {
            return Err("Username cannot be empty".to_string());
        }
        if creds.password.is_empty() {
            return Err("Password cannot be empty".to_string());
        }
        Ok(())
    }

    /// Build user permissions based on admin status (synchronous helper)
    pub fn build_user_permissions(is_admin: bool) -> HashSet<String> {
        let mut perms = HashSet::new();
        if is_admin {
            perms.insert("admin".into());
        }
        perms
    }

    /// Build group permissions (always empty) (synchronous helper)
    pub fn build_group_permissions() -> HashSet<String> {
        HashSet::new()
    }
}

/// Your Mongo‐backed auth manager
#[derive(Clone)]
pub struct MongoAuth {
    pub users: Collection<User>,
}

#[async_trait]
impl AuthnBackend for MongoAuth {
    type User = User;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(&self, creds: Credentials) -> Result<Option<Self::User>, Self::Error> {
        let maybe = self
            .users
            .find_one(doc! { "username": &creds.username })
            .await?;
        if let Some(u) = maybe {
            if verify_password(&creds.password, &u.password_hash).await {
                return Ok(Some(u));
            }
        }
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let user = self.users.find_one(doc! { "_id": user_id.clone() }).await?;
        Ok(user)
    }
}

#[async_trait]
impl AuthzBackend for MongoAuth {
    type Permission = String;

    fn get_user_permissions<'a, 'b, 'c>(
        &'a self,
        user: &'b User,
    ) -> Pin<Box<dyn Future<Output = Result<HashSet<String>, AuthError>> + Send + 'c>>
    where
        Self: Sync + 'c,
        'a: 'c,
        'b: 'c,
    {
        let is_admin = user.is_admin;
        Box::pin(async move {
            let mut perms = HashSet::new();
            if is_admin {
                perms.insert("admin".into());
            }
            Ok(perms)
        })
    }

    fn get_group_permissions<'a, 'b, 'c>(
        &'a self,
        _user: &'b User,
    ) -> Pin<Box<dyn Future<Output = Result<HashSet<String>, AuthError>> + Send + 'c>>
    where
        Self: Sync + 'c,
        'a: 'c,
        'b: 'c,
    {
        Box::pin(async { Ok(HashSet::new()) })
    }
}
