pub fn normalize_url(raw: &str) -> String {
    raw.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_basic() {
        assert_eq!(normalize_url("  HTTPS://AAA.COM/1  "), "https://aaa.com/1");
    }

    #[test]
    fn test_normalize_preserves_path_differences() {
        assert_ne!(
            normalize_url("https://aaa.com/1"),
            normalize_url("https://aaa.com/2")
        );
    }

    #[test]
    fn test_normalize_case_insensitive_match() {
        assert_eq!(
            normalize_url("https://AAA.COM/1"),
            normalize_url("https://aaa.com/1")
        );
    }
}
