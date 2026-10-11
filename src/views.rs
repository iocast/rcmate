pub mod about;
pub mod error;
pub mod main;
pub mod options;
pub mod progress;
pub mod settings_form;
pub mod sync_pair_form;

use crate::tui::View;

pub enum ViewAction {
    SwitchTo(View),
    OpenPopup(View),
    ClosePopup,
    Quit,
    None,
}

/// Turns text a terminal pasted for a file/folder drag & drop into a plain
/// path. Terminals don't agree on the format: GNOME/VTE wraps it in single
/// quotes, Windows Terminal in double quotes, Konsole/kitty may
/// backslash-escape spaces, and some emit a `file://` URI. Most also append
/// a trailing space or newline. Only the first line is kept.
pub fn parse_dropped_path(text: &str) -> String {
    let text = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");

    if let Some(rest) = text.strip_prefix("file://") {
        // Drop the (usually empty) host part, keep the absolute path.
        let path = rest.find('/').map_or(rest, |i| &rest[i..]);
        return percent_decode(path);
    }

    for quote in ['\'', '"'] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            let inner = &text[1..text.len() - 1];
            // Shell quoting of an embedded single quote: 'it'\''s'
            return if quote == '\'' { inner.replace("'\\''", "'") } else { inner.to_string() };
        }
    }

    // Backslash is the path separator on Windows, not an escape.
    if cfg!(windows) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::parse_dropped_path;

    #[test]
    fn single_quoted_with_trailing_space() {
        assert_eq!(parse_dropped_path("'/home/me/My Docs' "), "/home/me/My Docs");
        assert_eq!(parse_dropped_path("'/tmp/it'\\''s'"), "/tmp/it's");
    }

    #[test]
    fn double_quoted() {
        assert_eq!(parse_dropped_path("\"C:\\Users\\me\\My Docs\""), "C:\\Users\\me\\My Docs");
    }

    #[test]
    fn file_uri() {
        assert_eq!(parse_dropped_path("file:///home/me/My%20Docs\r\n"), "/home/me/My Docs");
        assert_eq!(parse_dropped_path("file://host/tmp/x"), "/tmp/x");
    }

    #[test]
    #[cfg(not(windows))]
    fn backslash_escaped() {
        assert_eq!(parse_dropped_path("/home/me/My\\ Docs\n"), "/home/me/My Docs");
    }

    #[test]
    fn plain_path() {
        assert_eq!(parse_dropped_path("  /tmp/data\n"), "/tmp/data");
    }
}
