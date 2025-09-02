use std::cell::RefCell;
use jmespath::Runtime;
use super::register_custom_functions;

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new({
        let mut rt = Runtime::new();
        rt.register_builtin_functions();
        register_custom_functions(&mut rt);
        rt
    });
}

/// Execute a closure with a thread-local Runtime instance.
pub fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|cell| {
        let mut rt = cell.borrow_mut();
        f(&mut rt)
    })
}


