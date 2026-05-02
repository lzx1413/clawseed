//! Tests for the create_memory factory function.

use clawseed_config::schema::MemoryConfig;

#[tokio::test]
async fn create_memory_sqlite_backend() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = MemoryConfig {
        backend: "sqlite".to_string(),
        ..Default::default()
    };

    let mem = clawseed_memory::create_memory(&config, tmp.path(), None).unwrap();
    assert_eq!(mem.name(), "sqlite");

    mem.store(
        "test",
        "hello",
        clawseed_api::memory_traits::MemoryCategory::Core,
        None,
    )
    .await
    .unwrap();
    let entry = mem.get("test").await.unwrap();
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().content, "hello");
}

#[tokio::test]
async fn create_memory_none_backend() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = MemoryConfig {
        backend: "none".to_string(),
        ..Default::default()
    };

    let mem = clawseed_memory::create_memory(&config, tmp.path(), None).unwrap();
    assert_eq!(mem.name(), "none");

    mem.store(
        "test",
        "hello",
        clawseed_api::memory_traits::MemoryCategory::Core,
        None,
    )
    .await
    .unwrap();
    assert!(mem.get("test").await.unwrap().is_none());
    assert_eq!(mem.count().await.unwrap(), 0);
}

#[tokio::test]
async fn create_memory_unknown_backend_defaults_to_sqlite() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = MemoryConfig {
        backend: "redis".to_string(),
        ..Default::default()
    };

    let mem = clawseed_memory::create_memory(&config, tmp.path(), None).unwrap();
    assert_eq!(mem.name(), "sqlite");
}

#[tokio::test]
async fn create_memory_with_storage_and_routes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = MemoryConfig {
        backend: "sqlite".to_string(),
        ..Default::default()
    };

    let mem = clawseed_memory::create_memory_with_storage_and_routes(
        &config,
        &clawseed_config::schema::ProvidersConfig::default(),
        None,
        tmp.path(),
        None,
    )
    .unwrap();

    assert_eq!(mem.name(), "sqlite");
}

#[test]
fn should_skip_autosave_content_always_false() {
    assert!(!clawseed_memory::should_skip_autosave_content(
        "any content"
    ));
    assert!(!clawseed_memory::should_skip_autosave_content(""));
}
