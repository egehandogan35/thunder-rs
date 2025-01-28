use std::collections::HashMap;
#[must_use]
pub fn parse_request_path(buffer: &[u8]) -> Option<String> {
    let buffer_str = std::str::from_utf8(buffer).ok()?;
    let end_of_request_line = buffer_str.find("\r\n")?;
    let request_line = &buffer_str[..end_of_request_line];
    let mut parts = request_line.split_whitespace();

    let method = parts.next()?;
    let path = parts.next()?;

    if !matches!(
        method,
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD"
    ) {
        return None;
    }
    if path == "/" {
        return Some("/".to_string());
    }

    Some(path.to_string())
}
#[must_use]
pub fn parse_query_params(path: &str) -> HashMap<String, Vec<String>> {
    path.split('?').nth(1).map_or(HashMap::new(), |query| {
        query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.split('=');
                if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                    let decoded_key = urlencoding::decode(key).ok()?.to_string();
                    let decoded_value = urlencoding::decode(value).ok()?.to_string();
                    Some((decoded_key, decoded_value))
                } else {
                    None
                }
            })
            .fold(HashMap::new(), |mut acc, (key, value)| {
                acc.entry(key).or_default().push(value);
                acc
            })
    })
}
