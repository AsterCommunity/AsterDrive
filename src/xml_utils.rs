#[inline]
pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map(|(_, local)| local).unwrap_or(name)
}

#[inline]
pub fn local_name_eq(actual: &str, expected: &str) -> bool {
    local_name(actual) == expected
}

#[inline]
pub fn local_name_eq_ignore_case(actual: &str, expected: &str) -> bool {
    local_name(actual).eq_ignore_ascii_case(expected)
}

#[inline]
pub fn non_empty_owned_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
