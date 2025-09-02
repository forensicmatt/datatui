//! Custom JMESPath runtime and functions
//!
//! This module exposes a reusable `Runtime` preloaded with built-in functions
//! and application-specific custom functions.

mod runtime;
mod functions;

pub use runtime::with_runtime;
pub use functions::register_custom_functions;

/// Create a new `Runtime` with built-in functions and our custom functions registered.
// Helper kept for callers that want a fresh instance rather than thread-local.
pub fn new_runtime() -> jmespath::Runtime {
    let mut rt = jmespath::Runtime::new();
    rt.register_builtin_functions();
    register_custom_functions(&mut rt);
    rt
}


