//! The update-motd hook script.
//!
//! On Ubuntu, pam_motd runs every executable in /etc/update-motd.d (via
//! run-parts, in lexical order) at login and shows their combined output.
//! motder installs one small script there that prints its pre-rendered
//! messages file. The script never runs motder itself, so logins keep working
//! even if the binary is removed or moved.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::store::write_atomic;

/// `99-` sorts after Ubuntu's own scripts, so messages appear last, right
/// above the shell prompt.
pub const HOOK_NAME: &str = "99-motder";

const MARKER: &str = "# Managed by motder";

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Missing,
    Installed,
    /// Ours, but with different contents or not executable.
    Outdated,
    /// A file with our name that motder did not write.
    Foreign,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Created,
    Updated,
    Unchanged,
}

pub fn script(rendered: &Path) -> io::Result<String> {
    let path = rendered.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "state directory path must be valid UTF-8",
        )
    })?;
    Ok(format!(
        "#!/bin/sh\n\
         {MARKER} (https://crates.io/crates/motder). Do not edit:\n\
         # this file is rewritten automatically. Use `motder` to add or dismiss\n\
         # messages, and `motder uninstall` to remove this hook.\n\
         f={}\n\
         [ -r \"$f\" ] && cat \"$f\"\n\
         exit 0\n",
        shell_quote(path)
    ))
}

pub fn status(hook: &Path, rendered: &Path) -> io::Result<Status> {
    let current = match fs::read(hook) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Status::Missing),
        Err(e) => return Err(e),
    };
    if !is_ours(&current) {
        return Ok(Status::Foreign);
    }
    let executable = fs::metadata(hook)?.permissions().mode() & 0o111 != 0;
    if executable && current == script(rendered)?.as_bytes() {
        Ok(Status::Installed)
    } else {
        Ok(Status::Outdated)
    }
}

pub fn install(hook: &Path, rendered: &Path) -> io::Result<Change> {
    let change = match status(hook, rendered)? {
        Status::Installed => return Ok(Change::Unchanged),
        Status::Foreign => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "file exists and was not created by motder; refusing to overwrite it",
            ));
        }
        Status::Missing => Change::Created,
        Status::Outdated => Change::Updated,
    };
    write_atomic(hook, script(rendered)?.as_bytes(), 0o755)?;
    Ok(change)
}

/// Remove the hook if motder owns it. Returns whether a file was removed.
pub fn uninstall(hook: &Path) -> io::Result<bool> {
    match fs::read(hook) {
        Ok(c) if is_ours(&c) => fs::remove_file(hook).map(|()| true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "file was not created by motder; leaving it in place",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn is_ours(contents: &[u8]) -> bool {
    String::from_utf8_lossy(contents)
        .lines()
        .take(3)
        .any(|l| l.starts_with(MARKER))
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(
            shell_quote("/var/lib/motder/motd"),
            "'/var/lib/motder/motd'"
        );
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn script_is_recognised_as_ours() {
        let s = script(Path::new("/var/lib/motder/motd")).unwrap();
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("f='/var/lib/motder/motd'"));
        assert!(is_ours(s.as_bytes()));
        assert!(!is_ours(b"#!/bin/sh\necho hi\n"));
    }
}
