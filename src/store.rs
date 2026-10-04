//! The on-disk message store.
//!
//! Format (UTF-8, line based, one record per line):
//!
//! ```text
//! # motder store v1
//! next-id<TAB>4
//! msg<TAB><id><TAB><created unix secs><TAB><escaped text>
//! ```
//!
//! Text escapes: `\\` backslash, `\n` newline, `\t` tab.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

const HEADER: &str = "# motder store v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Stable identifier, never reused.
    pub id: u64,
    /// Creation time, seconds since the Unix epoch.
    pub created: u64,
    pub text: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Store {
    next_id: u64,
    pub messages: Vec<Message>,
}

impl Store {
    /// Append a message; its index is the new length of the list.
    pub fn add(&mut self, text: String, created: u64) -> usize {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.messages.push(Message { id, created, text });
        self.messages.len()
    }

    /// Remove messages by 1-based index (the last one when `indices` is empty).
    /// Validates every index before removing anything. Returns the removed
    /// messages with their former index, in ascending index order.
    pub fn remove(&mut self, indices: &[usize]) -> Result<Vec<(usize, Message)>, String> {
        let len = self.messages.len();
        if len == 0 {
            return Err("there are no messages to dismiss".into());
        }
        let mut wanted: Vec<usize> = if indices.is_empty() {
            vec![len]
        } else {
            indices.to_vec()
        };
        wanted.sort_unstable();
        wanted.dedup();
        if let Some(bad) = wanted.iter().find(|&&i| i == 0 || i > len) {
            let range = if len == 1 {
                "the only index is 1".to_string()
            } else {
                format!("valid indices are 1-{len}")
            };
            return Err(format!("no message with index {bad} ({range})"));
        }
        let mut removed: Vec<(usize, Message)> = wanted
            .iter()
            .rev()
            .map(|&i| (i, self.messages.remove(i - 1)))
            .collect();
        removed.reverse();
        Ok(removed)
    }

    pub fn parse(input: &str) -> Result<Store, String> {
        let mut lines = input.lines().enumerate();
        match lines.next() {
            Some((_, HEADER)) => {}
            _ => return Err("unrecognised store format (missing header)".into()),
        }
        let mut store = Store::default();
        for (n, line) in lines {
            let n = n + 1;
            if line.is_empty() {
                continue;
            }
            let bad = || format!("malformed store at line {n}");
            let mut fields = line.splitn(4, '\t');
            match fields.next() {
                Some("next-id") => {
                    store.next_id = fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                }
                Some("msg") => {
                    let id = fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    let created = fields.next().and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                    let text = fields.next().map(unescape).ok_or_else(bad)?;
                    store.messages.push(Message { id, created, text });
                }
                _ => return Err(bad()),
            }
        }
        let max_id = store.messages.iter().map(|m| m.id).max().unwrap_or(0);
        store.next_id = store.next_id.max(max_id + 1);
        Ok(store)
    }

    pub fn serialize(&self) -> String {
        let mut out = format!("{HEADER}\nnext-id\t{}\n", self.next_id.max(1));
        for m in &self.messages {
            out.push_str(&format!(
                "msg\t{}\t{}\t{}\n",
                m.id,
                m.created,
                escape(&m.text)
            ));
        }
        out
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Clean up user input: normalise line endings, drop trailing whitespace on
/// each line and blank lines at either end. Returns `None` if nothing is left.
pub fn normalize(raw: &str) -> Result<Option<String>, String> {
    if raw.contains('\0') {
        return Err("message contains a NUL byte".into());
    }
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|l| !l.is_empty());
    let end = lines.iter().rposition(|l| !l.is_empty());
    Ok(match (start, end) {
        (Some(s), Some(e)) => Some(lines[s..=e].join("\n")),
        _ => None,
    })
}

/// Load the store; a missing file is an empty store.
pub fn load(path: &Path) -> io::Result<Store> {
    match fs::read_to_string(path) {
        Ok(s) => Store::parse(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(e),
    }
}

/// Write `contents` to `path` atomically (temp file + rename) with `mode`.
pub fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(&tmp)?;
        file.write_all(contents)?;
        // Apply the mode explicitly so a restrictive umask can't hide the file
        // from pam_motd or from non-root `motder list`.
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_special_characters() {
        let mut s = Store::default();
        s.add("plain".into(), 10);
        s.add("multi\nline\twith \\n literal".into(), 20);
        s.add("unicode ✓ ünïcödé".into(), 30);
        let parsed = Store::parse(&s.serialize()).unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut s = Store::default();
        s.add("a".into(), 0);
        s.add("b".into(), 0);
        s.remove(&[]).unwrap();
        s.add("c".into(), 0);
        let ids: Vec<u64> = s.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1, 3]);
        let reparsed = Store::parse(&s.serialize()).unwrap();
        assert_eq!(reparsed.next_id, 4);
    }

    #[test]
    fn remove_last_by_default() {
        let mut s = Store::default();
        for t in ["a", "b", "c"] {
            s.add(t.into(), 0);
        }
        let removed = s.remove(&[]).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].0, 3);
        assert_eq!(removed[0].1.text, "c");
    }

    #[test]
    fn remove_several_is_all_or_nothing() {
        let mut s = Store::default();
        for t in ["a", "b", "c", "d"] {
            s.add(t.into(), 0);
        }
        assert!(s.remove(&[1, 9]).is_err());
        assert_eq!(s.messages.len(), 4);
        let removed = s.remove(&[3, 1, 3]).unwrap();
        let got: Vec<(usize, &str)> = removed.iter().map(|(i, m)| (*i, m.text.as_str())).collect();
        assert_eq!(got, vec![(1, "a"), (3, "c")]);
        let left: Vec<&str> = s.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(left, vec!["b", "d"]);
    }

    #[test]
    fn remove_from_empty_fails() {
        assert!(Store::default().remove(&[]).is_err());
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(Store::parse("").is_err());
        assert!(Store::parse("{}").is_err());
        assert!(Store::parse(&format!("{HEADER}\nmsg\tx\t1\thi\n")).is_err());
        assert!(Store::parse(&format!("{HEADER}\nwhat\n")).is_err());
    }

    #[test]
    fn normalize_cleans_input() {
        assert_eq!(
            normalize("  \n\nhello  \r\n  world\n\n")
                .unwrap()
                .as_deref(),
            Some("hello\n  world")
        );
        assert_eq!(normalize(" \n \t\n").unwrap(), None);
        assert_eq!(normalize("a\n\nb").unwrap().as_deref(), Some("a\n\nb"));
        assert!(normalize("a\0b").is_err());
    }
}
