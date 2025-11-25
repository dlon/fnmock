use fnmock::mockable;

#[mockable]
fn fetch_data(url: &str) -> String {
    format!("Real data from {}", url)
}

pub struct A {
    x: i32,
}

#[mockable]
impl A {
    pub fn test(&self) -> i32 {
        self.x
    }

    pub async fn async_test(&self) -> i32 {
        self.x
    }
}

#[test]
fn test_fetch_data_mocked() {
    // Original behavior
    assert_eq!(fetch_data("test.com"), "Real data from test.com");

    let g = set_mock_fetch_data(|url| format!("Mocked data from {}", url));

    // Now returns mocked value
    assert_eq!(fetch_data("test.com"), "Mocked data from test.com");

    drop(g);

    // Back to behavior
    assert_eq!(fetch_data("test.com"), "Real data from test.com");
}

#[tokio::test]
async fn test_struct_mocked() {
    let a = A { x: 10 };

    // Original behavior
    assert_eq!(a.test(), 10);
    assert_eq!(a.async_test().await, 10);

    let _g = A::set_mock_test(|_self| 20);
    let _g = A::set_mock_async_test(|_self| 20);

    // Now returns mocked value
    assert_eq!(a.test(), 20);
    assert_eq!(a.async_test().await, 20);
}
