//! End-to-end tests: run the real binary against temporary directories.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

struct Env {
    root: PathBuf,
}

impl Env {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "motder-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("update-motd.d")).unwrap();
        Env { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }
    fn motd_dir(&self) -> PathBuf {
        self.root.join("update-motd.d")
    }
    fn hook(&self) -> PathBuf {
        self.motd_dir().join("99-motder")
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_motder"));
        c.args(args)
            .env("MOTDER_STATE_DIR", self.state())
            .env("MOTDER_MOTD_DIR", self.motd_dir())
            .stdin(Stdio::null());
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "motder {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn fail(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "motder {args:?} unexpectedly succeeded"
        );
        String::from_utf8(out.stderr).unwrap()
    }

    fn with_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut child = self
            .cmd(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// What a user would see at login from our hook.
    fn login_output(&self) -> String {
        let out = Command::new("/bin/sh").arg(self.hook()).output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    }

    fn texts(&self) -> Vec<String> {
        // Parse our own JSON just enough for assertions.
        self.ok(&["list", "--json"])
            .lines()
            .filter_map(|l| l.split("\"text\": ").nth(1))
            .map(|t| {
                t.trim_end_matches(',')
                    .trim_end_matches('}')
                    .trim_matches('"')
                    .to_string()
            })
            .collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn mode(p: &Path) -> u32 {
    fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn empty_list() {
    let env = Env::new();
    assert_eq!(env.ok(&[]), "No messages.\n");
    assert_eq!(env.ok(&["list", "--json"]), "[]\n");
    assert!(!env.state().exists(), "listing must not create state");
}

#[test]
fn add_installs_hook_and_shows_at_login() {
    let env = Env::new();
    assert_eq!(
        env.ok(&["add", "Disk", "/data", "is", "91%", "full"]),
        "Added message [1].\n"
    );
    assert_eq!(env.ok(&["add", "Reboot scheduled"]), "Added message [2].\n");

    assert_eq!(mode(&env.hook()), 0o755);
    assert_eq!(mode(&env.state().join("messages")), 0o644);
    assert_eq!(mode(&env.state().join("motd")), 0o644);

    assert_eq!(
        env.login_output(),
        "\n * Disk /data is 91% full\n * Reboot scheduled\n"
    );
    assert_eq!(env.ok(&["show"]), env.login_output());

    let list = env.ok(&["ls"]);
    assert!(list.starts_with("#  ADDED     MESSAGE\n"), "{list}");
    assert!(
        list.contains("1  just now  Disk /data is 91% full\n"),
        "{list}"
    );
    assert!(list.contains("2  just now  Reboot scheduled\n"), "{list}");
}

#[test]
fn dismiss_defaults_to_last() {
    let env = Env::new();
    for m in ["one", "two", "three"] {
        env.ok(&["add", m]);
    }
    assert_eq!(env.ok(&["dismiss"]), "Dismissed [3]: three\n");
    assert_eq!(env.texts(), ["one", "two"]);
    assert_eq!(env.login_output(), "\n * one\n * two\n");
}

#[test]
fn dismiss_by_index_and_reindex() {
    let env = Env::new();
    for m in ["a", "b", "c", "d"] {
        env.ok(&["add", m]);
    }
    assert_eq!(
        env.ok(&["rm", "3", "1"]),
        "Dismissed [1]: a\nDismissed [3]: c\n"
    );
    assert_eq!(env.texts(), ["b", "d"]);
    // Indices are positions, so "d" is now [2].
    assert_eq!(env.ok(&["dismiss", "2"]), "Dismissed [2]: d\n");
    assert_eq!(env.texts(), ["b"]);
}

#[test]
fn dismiss_errors_leave_store_untouched() {
    let env = Env::new();
    assert!(env.fail(&["dismiss"]).contains("no messages to dismiss"));
    env.ok(&["add", "x"]);
    env.ok(&["add", "y"]);
    let err = env.fail(&["dismiss", "1", "5"]);
    assert!(
        err.contains("no message with index 5 (valid indices are 1-2)"),
        "{err}"
    );
    assert_eq!(env.texts(), ["x", "y"]);
    let out = env.run(&["dismiss", "zero"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn clear_empties_login_output() {
    let env = Env::new();
    env.ok(&["add", "x"]);
    env.ok(&["add", "y"]);
    assert_eq!(env.ok(&["clear"]), "Dismissed 2 messages.\n");
    assert_eq!(env.login_output(), "");
    assert_eq!(env.ok(&["clear"]), "No messages.\n");
}

#[test]
fn add_from_stdin_multiline() {
    let env = Env::new();
    let out = env.with_stdin(&["add"], "\nBackup failed:\n  /srv/db\r\n\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = env.with_stdin(&["add", "-"], "second\n");
    assert!(out.status.success());
    assert_eq!(
        env.login_output(),
        "\n * Backup failed:\n     /srv/db\n * second\n"
    );
    let list = env.ok(&["list"]);
    assert!(
        list.contains("1  just now  Backup failed:\n               /srv/db\n"),
        "{list}"
    );
}

#[test]
fn rejects_empty_and_dash_messages() {
    let env = Env::new();
    let out = env.with_stdin(&["add"], "   \n\n");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("message is empty"));
    let out = env.run(&["add", "-oops"]);
    assert_eq!(out.status.code(), Some(2));
    env.ok(&["add", "--", "-5% headroom"]);
    assert_eq!(env.texts(), ["-5% headroom"]);
}

#[test]
fn json_output() {
    let env = Env::new();
    env.ok(&["add", "say \"hi\""]);
    let json = env.ok(&["list", "--json"]);
    assert!(
        json.starts_with("[\n  {\"index\": 1, \"id\": 1, \"created\": \""),
        "{json}"
    );
    assert!(json.contains(r#""text": "say \"hi\""}"#), "{json}");
}

#[test]
fn special_characters_survive_shell_hook() {
    let env = Env::new();
    let msg = "quote ' and \" $HOME `id` \\ backslash";
    env.ok(&["add", msg]);
    assert_eq!(env.login_output(), format!("\n * {msg}\n"));
}

#[test]
fn hook_is_repaired_and_foreign_files_are_respected() {
    let env = Env::new();
    env.ok(&["add", "x"]);

    // Lost its executable bit -> list warns, install repairs.
    fs::set_permissions(env.hook(), fs::Permissions::from_mode(0o644)).unwrap();
    let out = env.run(&["list"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("is out of date"));
    let out = env.run(&["install"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("Repaired hook"));
    assert_eq!(mode(&env.hook()), 0o755);
    assert!(env.ok(&["install"]).contains("already installed"));

    // A file motder didn't write is never overwritten or deleted.
    fs::write(env.hook(), "#!/bin/sh\necho mine\n").unwrap();
    assert!(env.fail(&["add", "y"]).contains("refusing to overwrite"));
    assert!(env.fail(&["uninstall"]).contains("leaving it in place"));
    assert_eq!(
        fs::read_to_string(env.hook()).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
}

#[test]
fn uninstall_keeps_or_purges_messages() {
    let env = Env::new();
    env.ok(&["add", "x"]);
    let out = env.ok(&["uninstall"]);
    assert!(out.contains("Removed hook"), "{out}");
    assert!(out.contains("Messages kept"), "{out}");
    assert!(!env.hook().exists());
    assert_eq!(env.texts(), ["x"]);
    assert!(String::from_utf8_lossy(&env.run(&["list"]).stderr).contains("is not installed"));

    env.ok(&["install"]);
    assert_eq!(env.login_output(), "\n * x\n");

    env.ok(&["uninstall", "--purge"]);
    assert!(!env.hook().exists());
    assert!(!env.state().exists());
    assert!(env.ok(&["uninstall"]).contains("Hook not installed"));
}

#[test]
fn missing_motd_dir_is_a_clear_error() {
    let env = Env::new();
    fs::remove_dir(env.motd_dir()).unwrap();
    let err = env.fail(&["add", "x"]);
    assert!(err.contains("does not exist"), "{err}");
    assert!(err.contains("--motd-dir"), "{err}");
}

#[test]
fn flags_override_env() {
    let env = Env::new();
    let other = env.root.join("other-state");
    let other_s = other.to_str().unwrap();
    env.ok(&["--state-dir", other_s, "add", "x"]);
    assert!(other.join("messages").exists());
    assert!(!env.state().exists());
    assert!(fs::read_to_string(env.hook()).unwrap().contains(other_s));
}

#[test]
fn concurrent_adds_do_not_lose_messages() {
    let env = Env::new();
    let children: Vec<_> = (0..16)
        .map(|i| {
            env.cmd(&["add", &format!("m{i}")])
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success());
    }
    assert_eq!(env.texts().len(), 16);
}

#[test]
fn help_and_version() {
    let env = Env::new();
    assert!(env.ok(&["--help"]).contains("dismiss, rm [INDEX]..."));
    assert_eq!(
        env.ok(&["--version"]),
        format!("motder {}\n", env!("CARGO_PKG_VERSION"))
    );
    let out = env.run(&["bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command 'bogus'"));
}
