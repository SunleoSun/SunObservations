pub fn segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_lowercase())
        && bytes.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
pub fn namespace(value: &str) -> bool {
    value.split("::").all(segment)
}

#[cfg(test)]
#[path = "validation_test.rs"]
mod tests;
