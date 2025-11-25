use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

// Re-export the macro from fnmock-macro
pub use fnmock_macro::mockable;

pub struct MockWrapper<F: ?Sized>(pub Arc<F>);

// Global mock storage
static MOCKS: LazyLock<Mutex<HashMap<String, Box<dyn std::any::Any + Send>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Guard that removes a mock when dropped.
/// This ensures mocks are automatically cleaned up at the end of a test.
#[must_use = "MockGuard must be held for the duration of the mock"]
pub struct MockGuard {
    name: String,
}

impl MockGuard {
    fn new(name: String) -> Self {
        Self { name }
    }

    /// Name/key of this mock
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for MockGuard {
    fn drop(&mut self) {
        let mut mocks = MOCKS.lock().unwrap();
        mocks.remove(&self.name);
    }
}

/// MockRegistry is used to register and retrieve mocks for functions
pub struct MockRegistry;

impl MockRegistry {
    /// Set a mock that's already wrapped in Arc (used by macro-generated helper)
    /// Returns a guard that removes the mock when dropped
    pub fn set_mock<F: ?Sized>(name: &str, mock: Arc<F>) -> MockGuard
    where
        F: 'static + Send + Sync,
    {
        let mut mocks = MOCKS.lock().unwrap();
        let wrapped = MockWrapper(mock);
        mocks.insert(name.to_string(), Box::new(wrapped));
        MockGuard::new(name.to_string())
    }

    /// Get a mock for a specific function
    /// F should be the trait object type (dyn Fn(...) -> Ret)
    pub fn get_mock<F>(name: &str) -> Option<Arc<F>>
    where
        F: ?Sized + Send + Sync + 'static,
    {
        let mocks = MOCKS.lock().unwrap();
        mocks
            .get(name)
            .and_then(|boxed| boxed.downcast_ref::<MockWrapper<F>>())
            .map(|wrapper| Arc::clone(&wrapper.0))
    }
}
