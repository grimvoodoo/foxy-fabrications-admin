use crate::{auth::MongoAuth, models::UserState};
use axum_login::AuthSession;

/// Extract user state from authentication session
pub fn extract_user_state(auth: &AuthSession<MongoAuth>) -> UserState {
    match auth.user {
        Some(ref user) => UserState::new(
            true, // is_authenticated
            user.username.clone(),
            user.is_admin,
        ),
        None => UserState::default(),
    }
}

#[cfg(test)]
mod tests {
    use crate::models::UserState;
    
    #[test]
    fn test_user_state_creation() {
        let user_state = UserState::new(true, "test".to_string(), true);
        
        assert!(user_state.logged_in);
        assert!(user_state.is_admin);
        assert_eq!(user_state.username, "test");
    }
    
    #[test]
    fn test_user_state_default() {
        let user_state = UserState::default();
        
        assert!(!user_state.logged_in);
        assert!(!user_state.is_admin);
        assert_eq!(user_state.username, "");
    }
    
    // Note: Testing extract_user_state would require complex mocking of AuthSession
    // which is difficult due to the concrete types. The function is tested indirectly
    // through integration tests and real usage.
}
