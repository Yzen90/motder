//! Rendering messages for the login screen and for `motder list`.

use crate::store::Message;
use crate::time;

/// The text shown at login, in the same " * " bullet style used by Ubuntu's
/// own update-motd scripts. Empty when there are no messages, so the hook
/// prints nothing at all.
pub fn motd(messages: &[Message]) -> String {
    if messages.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n");
    for m in messages {
        for (i, line) in m.text.lines().enumerate() {
            if i == 0 {
                out.push_str(" * ");
            } else if !line.is_empty() {
                out.push_str("   ");
            }
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Human-readable table for `motder list`.
pub fn table(messages: &[Message], now: u64) -> String {
    let ages: Vec<String> = messages
        .iter()
        .map(|m| time::relative(m.created, now))
        .collect();
    let idx_w = messages.len().to_string().len().max(1);
    let age_w = ages
        .iter()
        .map(String::len)
        .max()
        .unwrap_or(0)
        .max("ADDED".len());
    let indent = " ".repeat(idx_w + 2 + age_w + 2);

    let mut out = format!("{:>idx_w$}  {:<age_w$}  MESSAGE\n", "#", "ADDED");
    for (i, (m, age)) in messages.iter().zip(&ages).enumerate() {
        for (n, line) in m.text.lines().enumerate() {
            if n == 0 {
                out.push_str(&format!("{:>idx_w$}  {age:<age_w$}  {line}", i + 1));
            } else if !line.is_empty() {
                out.push_str(&indent);
                out.push_str(line);
            }
            out.push('\n');
        }
    }
    out
}

/// JSON array for `motder list --json`.
pub fn json(messages: &[Message]) -> String {
    if messages.is_empty() {
        return "[]\n".into();
    }
    let items: Vec<String> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            format!(
                "  {{\"index\": {}, \"id\": {}, \"created\": \"{}\", \"text\": {}}}",
                i + 1,
                m.id,
                time::iso8601(m.created),
                json_string(&m.text)
            )
        })
        .collect();
    format!("[\n{}\n]\n", items.join(",\n"))
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// First line of a message, shortened for one-line confirmations.
pub fn summary(text: &str) -> String {
    const MAX: usize = 60;
    let first = text.lines().next().unwrap_or("");
    let multiline = text.lines().nth(1).is_some();
    if first.chars().count() > MAX {
        let cut: String = first.chars().take(MAX - 1).collect();
        format!("{cut}…")
    } else if multiline {
        format!("{first} …")
    } else {
        first.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: u64, text: &str) -> Message {
        Message {
            id,
            created: 0,
            text: text.into(),
        }
    }

    #[test]
    fn motd_empty_prints_nothing() {
        assert_eq!(motd(&[]), "");
    }

    #[test]
    fn motd_bullets_and_continuations() {
        let out = motd(&[msg(1, "first"), msg(2, "second\nmore\n\nend")]);
        assert_eq!(out, "\n * first\n * second\n   more\n\n   end\n");
    }

    #[test]
    fn table_aligns_columns() {
        let ms: Vec<Message> = (1..=10).map(|i| msg(i, &format!("m{i}"))).collect();
        let out = table(&ms, 0);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], " #  ADDED     MESSAGE");
        assert_eq!(lines[1], " 1  just now  m1");
        assert_eq!(lines[10], "10  just now  m10");
    }

    #[test]
    fn json_escapes() {
        let out = json(&[msg(7, "a \"q\"\nb\\")]);
        assert!(out.contains(r#""text": "a \"q\"\nb\\""#), "{out}");
        assert!(out.contains(r#""index": 1, "id": 7, "created": "1970-01-01T00:00:00Z""#));
        assert_eq!(json(&[]), "[]\n");
    }

    #[test]
    fn summary_truncates() {
        assert_eq!(summary("short"), "short");
        assert_eq!(summary("a\nb"), "a …");
        let long = "x".repeat(100);
        assert_eq!(summary(&long).chars().count(), 60);
    }
}
