use url::Url;

/// Derive a default remark from a subscription URL's hostname.
///
/// Rules from spec:
/// - `aaa.com` -> `aaa`
/// - `aa.bbb.ccc` -> `bbb`
/// - fallback: normalized host
pub fn derive_remark(subscription_url: &str) -> String {
    let Ok(parsed) = Url::parse(subscription_url) else {
        return sanitize_fallback(subscription_url);
    };

    let Some(host_str) = parsed.host_str() else {
        return sanitize_fallback(subscription_url);
    };
    let host = host_str.to_lowercase();

    let parts: Vec<&str> = host.split('.').collect();

    match parts.len() {
        2 => parts[0].to_string(),
        n if n >= 3 => parts[n - 2].to_string(),
        _ => sanitize_fallback(&host),
    }
}

fn sanitize_fallback(input: &str) -> String {
    let s: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() {
        "subscription".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_part_host() {
        assert_eq!(derive_remark("https://aaa.com/sub"), "aaa");
    }

    #[test]
    fn test_three_part_host() {
        assert_eq!(derive_remark("https://aa.bbb.ccc/sub"), "bbb");
    }

    #[test]
    fn test_four_part_host() {
        assert_eq!(derive_remark("https://x.y.example.com/sub"), "example");
    }

    #[test]
    fn test_fallback_on_ip() {
        let r = derive_remark("https://192.168.1.1/sub");
        assert!(!r.is_empty());
    }

    #[test]
    fn test_fallback_on_bad_url() {
        let r = derive_remark("not a url");
        assert!(!r.is_empty());
    }
}
