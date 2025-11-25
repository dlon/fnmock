# fnmock

A simple function mocking library for Rust tests.

## Usage

```rust
use fnmock::mockable;

#[mockable]
fn get_user_name() -> String {
    "Alice".to_string()
}

#[mockable]
fn add(a: i32, b: i32) -> i32 {
    a + b
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_mock() {
        assert_eq!(get_user_name(), "Alice");

        // Set mock using the generated helper function
        let _guard = set_mock_get_user_name(|| "Bob".to_string());

        // Function returns mocked value
        assert_eq!(get_user_name(), "Bob");
    }

    #[test]
    fn test_scoped_mock() {
        assert_eq!(add(2, 3), 5);

        {
            let _guard = set_mock_add(|a, b| a * 10 + b);
            assert_eq!(add(2, 3), 23);
        }

        // Back to original behavior
        assert_eq!(add(2, 3), 5);
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
}
```

## Limitations

- Parallel test execution is broken at the moment. I.e., multiple tests will interfere with each
  other if they mock the same function. `cargo test -- --test-threads=1` can be used to work around
  this.
