use std::sync::Arc;

use mewcode_engine::context::MemoryStore;
use uuid::Uuid;

use mewcode_server::ServerConfig;
use mewcode_server::store::{self, memory::MemoryStore as SessionMemoryStore};

fn test_state() -> mewcode_server::AppState {
    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: Some("test-key".into()),
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
        github: Default::default(),
    };
    let memory = MemoryStore::new(std::env::temp_dir().join(Uuid::new_v4().to_string()));
    mewcode_server::AppState::new(config, Arc::new(SessionMemoryStore::new()), memory)
}

#[tokio::test]
async fn session_operation_locks_serialize_only_matching_sessions() {
    let state = test_state();
    let session_id = Uuid::new_v4();
    let same_a = state.session_operation_lock(session_id).await;
    let same_b = state.session_operation_lock(session_id).await;
    let other = state.session_operation_lock(Uuid::new_v4()).await;

    assert!(Arc::ptr_eq(&same_a, &same_b));
    assert!(!Arc::ptr_eq(&same_a, &other));

    let _guard = same_a.lock().await;
    assert!(same_b.try_lock().is_err());
    assert!(other.try_lock().is_ok());
}

#[tokio::test]
async fn missing_session_does_not_allocate_operation_lock() {
    let state = test_state();

    let result = state.existing_session_operation_lock(Uuid::new_v4()).await;

    assert!(matches!(result, Err(store::StoreError::NotFound)));
    assert!(state.session_operations.lock().await.is_empty());
}

#[test]
fn operation_lock_cleanup_only_follows_not_found() {
    assert!(mewcode_server::should_remove_operation_lock(
        &store::StoreError::NotFound
    ));
    assert!(!mewcode_server::should_remove_operation_lock(
        &store::StoreError::Invalid("transient".into())
    ));
    assert!(!mewcode_server::should_remove_operation_lock(
        &store::StoreError::Io(std::io::Error::other("transient"))
    ));
}

#[tokio::test]
async fn transient_error_lock_is_removed_only_without_waiters() {
    let state = test_state();
    let session_id = Uuid::new_v4();
    let lock = state.session_operation_lock(session_id).await;
    let waiter = Arc::clone(&lock);

    state
        .remove_uncontended_session_operation_lock(session_id, &lock)
        .await;
    assert!(
        state
            .session_operations
            .lock()
            .await
            .contains_key(&session_id)
    );

    drop(waiter);
    state
        .remove_uncontended_session_operation_lock(session_id, &lock)
        .await;
    assert!(
        !state
            .session_operations
            .lock()
            .await
            .contains_key(&session_id)
    );
}
