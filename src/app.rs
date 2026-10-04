//! Command implementations.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::cli::{Cli, Command, HELP};
use crate::error::{Context, Error, Result};
use crate::hook::{self, Change};
use crate::render;
use crate::store::{self, Store};
use crate::time;

struct Paths {
    state_dir: PathBuf,
    motd_dir: PathBuf,
}

impl Paths {
    fn store(&self) -> PathBuf {
        self.state_dir.join("messages")
    }
    fn rendered(&self) -> PathBuf {
        self.state_dir.join("motd")
    }
    fn lock(&self) -> PathBuf {
        self.state_dir.join("lock")
    }
    fn hook(&self) -> PathBuf {
        self.motd_dir.join(hook::HOOK_NAME)
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let paths = Paths {
        state_dir: cli.state_dir,
        motd_dir: cli.motd_dir,
    };
    match cli.command {
        Command::Help => print(HELP),
        Command::Version => print(&format!("motder {}\n", env!("CARGO_PKG_VERSION"))),
        Command::Add { words } => add(&paths, words),
        Command::List { json } => list(&paths, json),
        Command::Dismiss { indices } => dismiss(&paths, &indices),
        Command::Clear => clear(&paths),
        Command::Show => show(&paths),
        Command::Install => install(&paths),
        Command::Uninstall { purge } => uninstall(&paths, purge),
    }
}

fn add(paths: &Paths, words: Vec<String>) -> Result<()> {
    let raw = if words.is_empty() || words == ["-"] {
        let stdin = io::stdin();
        if words.is_empty() && stdin.is_terminal() {
            return Err(Error::msg(
                "no message given; pass it as arguments or pipe it on stdin (see 'motder --help')",
            ));
        }
        let mut buf = Vec::new();
        stdin
            .lock()
            .read_to_end(&mut buf)
            .context(|| "reading message from stdin".into())?;
        String::from_utf8(buf).map_err(|_| Error::msg("message on stdin is not valid UTF-8"))?
    } else {
        words.join(" ")
    };

    let text = store::normalize(&raw)
        .map_err(Error::Msg)?
        .ok_or_else(|| Error::msg("message is empty"))?;

    let (index, change) = mutate(paths, |s| Ok(s.add(text, time::now())))?;
    report_hook(paths, &change);
    print(&format!("Added message [{index}].\n"))
}

fn list(paths: &Paths, json: bool) -> Result<()> {
    let store = load(paths)?;
    if json {
        return print(&render::json(&store.messages));
    }
    if store.messages.is_empty() {
        print("No messages.\n")?;
    } else {
        print(&render::table(&store.messages, time::now()))?;
        warn_if_hook_inactive(paths);
    }
    Ok(())
}

fn dismiss(paths: &Paths, indices: &[usize]) -> Result<()> {
    let (removed, _) = mutate(paths, |s| s.remove(indices).map_err(Error::Msg))?;
    let mut out = String::new();
    for (index, m) in &removed {
        out.push_str(&format!(
            "Dismissed [{index}]: {}\n",
            render::summary(&m.text)
        ));
    }
    print(&out)
}

fn clear(paths: &Paths) -> Result<()> {
    let (count, _) = mutate(paths, |s| Ok(s.messages.drain(..).count()))?;
    match count {
        0 => print("No messages.\n"),
        1 => print("Dismissed 1 message.\n"),
        n => print(&format!("Dismissed {n} messages.\n")),
    }
}

fn show(paths: &Paths) -> Result<()> {
    let store = load(paths)?;
    if store.messages.is_empty() {
        if io::stdout().is_terminal() {
            eprintln!("(no messages; nothing is shown at login)");
        }
        return Ok(());
    }
    print(&render::motd(&store.messages))
}

fn install(paths: &Paths) -> Result<()> {
    let (_, change) = mutate(paths, |_| Ok(()))?;
    if change == Change::Unchanged {
        print(&format!(
            "Hook already installed: {}\n",
            paths.hook().display()
        ))
    } else {
        report_hook(paths, &change);
        Ok(())
    }
}

fn uninstall(paths: &Paths, purge: bool) -> Result<()> {
    let hook = paths.hook();
    let removed = hook::uninstall(&hook).context(|| format!("removing {}", hook.display()))?;
    let mut out = if removed {
        format!("Removed hook {}\n", hook.display())
    } else {
        format!("Hook not installed ({})\n", hook.display())
    };

    if purge {
        for file in [paths.store(), paths.rendered(), paths.lock()] {
            remove_if_exists(&file)?;
        }
        // Only removes the directory if nothing else lives there.
        match fs::remove_dir(&paths.state_dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {}
            Err(e) => {
                return Err(e).context(|| format!("removing {}", paths.state_dir.display()));
            }
        }
        out.push_str(&format!(
            "Deleted stored messages in {}\n",
            paths.state_dir.display()
        ));
    } else if load(paths).map(|s| !s.messages.is_empty()).unwrap_or(false) {
        out.push_str(&format!(
            "Messages kept in {}; 'motder install' restores the hook, \
             'motder uninstall --purge' deletes them.\n",
            paths.state_dir.display()
        ));
    }
    print(&out)
}

/// Run `f` on the store under an exclusive lock, then persist the store,
/// re-render the login text and make sure the hook is in place.
fn mutate<R>(paths: &Paths, f: impl FnOnce(&mut Store) -> Result<R>) -> Result<(R, Change)> {
    check_motd_dir(&paths.motd_dir)?;
    ensure_dir(&paths.state_dir)?;
    let _lock = lock(&paths.lock())?;

    let mut store = load(paths)?;
    let result = f(&mut store)?;

    let store_file = paths.store();
    store::write_atomic(&store_file, store.serialize().as_bytes(), 0o644)
        .context(|| format!("writing {}", store_file.display()))?;

    let rendered = paths.rendered();
    store::write_atomic(&rendered, render::motd(&store.messages).as_bytes(), 0o644)
        .context(|| format!("writing {}", rendered.display()))?;

    let hook = paths.hook();
    let change = hook::install(&hook, &rendered)
        .context(|| format!("installing hook {}", hook.display()))?;

    Ok((result, change))
}

fn load(paths: &Paths) -> Result<Store> {
    let file = paths.store();
    store::load(&file).context(|| format!("reading {}", file.display()))
}

fn check_motd_dir(dir: &Path) -> Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    Err(Error::msg(format!(
        "{} does not exist; motder needs Ubuntu's update-motd system \
         (package 'update-motd' / pam_motd). Use --motd-dir to point elsewhere.",
        dir.display()
    )))
}

fn ensure_dir(dir: &Path) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(dir)
        .context(|| format!("creating {}", dir.display()))
}

/// Exclusive advisory lock, released when the returned file is dropped.
fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o644)
        .open(path)
        .context(|| format!("opening {}", path.display()))?;
    file.lock()
        .context(|| format!("locking {}", path.display()))?;
    Ok(file)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(e).context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}

fn report_hook(paths: &Paths, change: &Change) {
    let hook = paths.hook();
    match change {
        Change::Created => eprintln!("Installed hook {}", hook.display()),
        Change::Updated => eprintln!("Repaired hook {}", hook.display()),
        Change::Unchanged => {}
    }
}

fn warn_if_hook_inactive(paths: &Paths) {
    let hook = paths.hook();
    let problem = match hook::status(&hook, &paths.rendered()) {
        Ok(hook::Status::Installed) => return,
        Ok(hook::Status::Missing) => "is not installed",
        Ok(hook::Status::Outdated) => "is out of date",
        Ok(hook::Status::Foreign) => "is occupied by a file motder did not create",
        Err(_) => return,
    };
    eprintln!(
        "warning: hook {} {problem}, so these messages are not shown at login; \
         run 'sudo motder install'",
        hook.display()
    );
}

/// Write to stdout, treating a closed pipe (e.g. `motder list | head`) as success.
fn print(s: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(s.as_bytes()).and_then(|()| out.flush()) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        r => r.context(|| "writing to stdout".into()),
    }
}
