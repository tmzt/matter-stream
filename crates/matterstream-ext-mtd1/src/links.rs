//! Markdown link extraction for mtd1 content.
//!
//! Parses `[text](url)` patterns from text, returning plain text (with
//! markdown syntax stripped) and link spans with byte ranges + URLs.
//!
//! This layer sits between content and `pretext_rs` — the layout engine
//! sees only plain text, and the compiler uses the spans to emit
//! `ActionRegion` entries after layout.

/// A link span in the plain text output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSpan {
    /// Start byte offset in the plain text (markdown stripped).
    pub start: usize,
    /// End byte offset in the plain text (exclusive).
    pub end: usize,
    /// The URL target (e.g. "card-open:gmail-message:abc123").
    pub url: String,
}

/// Strip markdown links from text, returning plain text and link spans.
///
/// Only handles `[text](url)` syntax — no other markdown.
/// Nested brackets and escaped brackets are not supported.
///
/// # Example
/// ```
/// use matterstream_ext_mtd1::links::extract_links;
/// let (plain, links) = extract_links("Check [Invoice](card-open:gmail-message:abc) now");
/// assert_eq!(plain, "Check Invoice now");
/// assert_eq!(links.len(), 1);
/// assert_eq!(links[0].start, 6);
/// assert_eq!(links[0].end, 13);
/// assert_eq!(links[0].url, "card-open:gmail-message:abc");
/// ```
pub fn extract_links(text: &str) -> (String, Vec<LinkSpan>) {
    let mut plain = String::with_capacity(text.len());
    let mut links = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Try to parse [text](url)
            if let Some((link_text, url, consumed)) = parse_link(&bytes[i..]) {
                let start = plain.len();
                plain.push_str(link_text);
                let end = plain.len();
                links.push(LinkSpan {
                    start,
                    end,
                    url: url.to_string(),
                });
                i += consumed;
                continue;
            }
        }
        plain.push(bytes[i] as char);
        i += 1;
    }

    (plain, links)
}

/// Try to parse `[text](url)` starting at `data[0] == '['`.
/// Returns (link_text, url, total_bytes_consumed) or None.
fn parse_link(data: &[u8]) -> Option<(&str, &str, usize)> {
    if data.is_empty() || data[0] != b'[' {
        return None;
    }

    // Find closing ]
    let close_bracket = data[1..].iter().position(|&b| b == b']')? + 1;

    // Must be followed by (
    if close_bracket + 1 >= data.len() || data[close_bracket + 1] != b'(' {
        return None;
    }

    // Find closing )
    let url_start = close_bracket + 2;
    let close_paren = data[url_start..].iter().position(|&b| b == b')')? + url_start;

    let link_text = std::str::from_utf8(&data[1..close_bracket]).ok()?;
    let url = std::str::from_utf8(&data[url_start..close_paren]).ok()?;

    Some((link_text, url, close_paren + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_links() {
        let (plain, links) = extract_links("Hello world");
        assert_eq!(plain, "Hello world");
        assert!(links.is_empty());
    }

    #[test]
    fn single_link() {
        let (plain, links) = extract_links("[Click here](https://example.com)");
        assert_eq!(plain, "Click here");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].start, 0);
        assert_eq!(links[0].end, 10);
        assert_eq!(links[0].url, "https://example.com");
    }

    #[test]
    fn link_in_sentence() {
        let (plain, links) = extract_links("See [Invoice #1234](card-open:gmail-message:abc) for details");
        assert_eq!(plain, "See Invoice #1234 for details");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].start, 4);
        assert_eq!(links[0].end, 17);
        assert_eq!(links[0].url, "card-open:gmail-message:abc");
    }

    #[test]
    fn multiple_links() {
        let (plain, links) = extract_links("[A](url1) and [B](url2)");
        assert_eq!(plain, "A and B");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], LinkSpan { start: 0, end: 1, url: "url1".into() });
        assert_eq!(links[1], LinkSpan { start: 6, end: 7, url: "url2".into() });
    }

    #[test]
    fn bare_brackets_not_link() {
        let (plain, links) = extract_links("array[0] = value");
        assert_eq!(plain, "array[0] = value");
        assert!(links.is_empty());
    }

    #[test]
    fn incomplete_link() {
        let (plain, links) = extract_links("[text] no url");
        assert_eq!(plain, "[text] no url");
        assert!(links.is_empty());
    }

    #[test]
    fn card_open_url() {
        let (plain, links) = extract_links("[Q1 Budget](card-open:drive-file:1abc2def)");
        assert_eq!(plain, "Q1 Budget");
        assert_eq!(links[0].url, "card-open:drive-file:1abc2def");
    }

    #[test]
    fn empty_text() {
        let (plain, links) = extract_links("");
        assert_eq!(plain, "");
        assert!(links.is_empty());
    }

    #[test]
    fn adjacent_links() {
        let (plain, links) = extract_links("[A](u1)[B](u2)");
        assert_eq!(plain, "AB");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], LinkSpan { start: 0, end: 1, url: "u1".into() });
        assert_eq!(links[1], LinkSpan { start: 1, end: 2, url: "u2".into() });
    }
}
