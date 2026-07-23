use std::io::Write;

pub(super) fn buffered<T>(run: impl FnOnce(&mut Vec<u8>) -> T) -> T {
    let mut diagnostic = Vec::new();
    let result = run(&mut diagnostic);
    if !diagnostic.is_empty() {
        let _ = std::io::stderr().lock().write_all(&diagnostic);
    }
    result
}
