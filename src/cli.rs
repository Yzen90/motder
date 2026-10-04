//! Command-line parsing (std only, no external parser).

use std::ffi::OsString;
use std::path::PathBuf;

pub const DEFAULT_STATE_DIR: &str = "/var/lib/motder";
pub const DEFAULT_MOTD_DIR: &str = "/etc/update-motd.d";

pub const HELP: &str = "\
motder - add, list and dismiss messages in Ubuntu's message of the day

USAGE:
    motder [OPTIONS] [COMMAND] [ARGS]

COMMANDS:
    add <MESSAGE>...       Add a message (words are joined with spaces).
                           With no MESSAGE, or '-', the message is read from stdin.
    list, ls [--json]      List current messages with their index (default command).
    dismiss, rm [INDEX]... Dismiss messages by index; with no INDEX, the last one.
    clear                  Dismiss all messages.
    show                   Print the motder section exactly as it appears at login.
    install                Install (or repair) the update-motd hook.
    uninstall [--purge]    Remove the hook; --purge also deletes stored messages.
    help                   Show this help.

OPTIONS:
    --state-dir <DIR>      Where messages are stored
                           [env: MOTDER_STATE_DIR] [default: /var/lib/motder]
    --motd-dir <DIR>       The update-motd script directory
                           [env: MOTDER_MOTD_DIR] [default: /etc/update-motd.d]
    -h, --help             Show this help.
    -V, --version          Show the version.

Use '--' to pass a message that starts with '-': motder add -- '-5% CPU headroom'

Changing messages requires root (sudo). Messages appear on the next login;
'sudo run-parts /etc/update-motd.d' previews the full message of the day.

EXAMPLES:
    sudo motder add 'Maintenance window: Saturday 02:00-04:00 UTC'
    df -h /data | sudo motder add
    motder list
    sudo motder dismiss        # dismiss the most recent message
    sudo motder dismiss 1 3    # dismiss messages 1 and 3
";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Add { words: Vec<String> },
    List { json: bool },
    Dismiss { indices: Vec<usize> },
    Clear,
    Show,
    Install,
    Uninstall { purge: bool },
    Help,
    Version,
}

#[derive(Debug)]
pub struct Cli {
    pub state_dir: PathBuf,
    pub motd_dir: PathBuf,
    pub command: Command,
}

/// Parse arguments (without the program name). `env` looks up environment
/// variables, injected so parsing can be tested deterministically.
pub fn parse<I, F>(args: I, env: F) -> Result<Cli, String>
where
    I: IntoIterator<Item = OsString>,
    F: Fn(&str) -> Option<OsString>,
{
    let from_env = |key: &str| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut state_dir = from_env("MOTDER_STATE_DIR");
    let mut motd_dir = from_env("MOTDER_MOTD_DIR");

    let mut command: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut literal = false;
    let mut want_help = false;
    let mut want_version = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = utf8(arg)?;

        if literal {
            if command.is_none() {
                command = Some(arg);
            } else {
                positional.push(arg);
            }
            continue;
        }

        if arg == "--" {
            literal = true;
            continue;
        }

        if let Some(value) = option_value("--state-dir", &arg, &mut iter)? {
            state_dir = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = option_value("--motd-dir", &arg, &mut iter)? {
            motd_dir = Some(PathBuf::from(value));
            continue;
        }

        match arg.as_str() {
            "-h" | "--help" => want_help = true,
            "-V" | "--version" => want_version = true,
            "-" => positional.push(arg),
            _ if arg.starts_with('-') => flags.push(arg),
            _ if command.is_none() => command = Some(arg),
            _ => positional.push(arg),
        }
    }

    let command = if want_help {
        Command::Help
    } else if want_version {
        Command::Version
    } else {
        build_command(command.as_deref(), positional, flags)?
    };

    Ok(Cli {
        state_dir: state_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_STATE_DIR)),
        motd_dir: motd_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_MOTD_DIR)),
        command,
    })
}

fn build_command(
    name: Option<&str>,
    positional: Vec<String>,
    flags: Vec<String>,
) -> Result<Command, String> {
    let name = name.unwrap_or("list");
    let cmd = match name {
        "add" => {
            if let Some(flag) = flags.first() {
                return Err(format!(
                    "unknown option '{flag}' for 'add' (use '--' before a message that starts with '-')"
                ));
            }
            return Ok(Command::Add { words: positional });
        }
        "list" | "ls" => {
            no_positional(name, &positional)?;
            let json = take_flags(name, flags, &["--json"])?.contains(&"--json");
            return Ok(Command::List { json });
        }
        "dismiss" | "rm" => {
            take_flags(name, flags, &[])?;
            let indices = positional
                .iter()
                .map(|s| parse_index(s))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Command::Dismiss { indices });
        }
        "uninstall" => {
            no_positional(name, &positional)?;
            let purge = take_flags(name, flags, &["--purge"])?.contains(&"--purge");
            return Ok(Command::Uninstall { purge });
        }
        "clear" => Command::Clear,
        "show" => Command::Show,
        "install" => Command::Install,
        "help" => Command::Help,
        "version" => Command::Version,
        other => return Err(format!("unknown command '{other}'")),
    };
    no_positional(name, &positional)?;
    take_flags(name, flags, &[])?;
    Ok(cmd)
}

fn utf8(arg: OsString) -> Result<String, String> {
    arg.into_string()
        .map_err(|a| format!("argument is not valid UTF-8: {}", a.to_string_lossy()))
}

/// Recognise `--name VALUE` and `--name=VALUE`.
fn option_value<I: Iterator<Item = OsString>>(
    name: &str,
    arg: &str,
    rest: &mut I,
) -> Result<Option<String>, String> {
    if arg == name {
        let value = rest
            .next()
            .ok_or_else(|| format!("option '{name}' needs a value"))?;
        let value = utf8(value)?;
        if value.is_empty() {
            return Err(format!("option '{name}' needs a non-empty value"));
        }
        return Ok(Some(value));
    }
    if let Some(value) = arg.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
        if value.is_empty() {
            return Err(format!("option '{name}' needs a non-empty value"));
        }
        return Ok(Some(value.to_owned()));
    }
    Ok(None)
}

fn no_positional(cmd: &str, positional: &[String]) -> Result<(), String> {
    match positional.first() {
        Some(extra) => Err(format!("unexpected argument '{extra}' for '{cmd}'")),
        None => Ok(()),
    }
}

/// Check that every flag is one of `allowed`; returns the matching allowed names.
fn take_flags(
    cmd: &str,
    flags: Vec<String>,
    allowed: &[&'static str],
) -> Result<Vec<&'static str>, String> {
    flags
        .iter()
        .map(|f| {
            allowed
                .iter()
                .copied()
                .find(|a| a == f)
                .ok_or_else(|| format!("unknown option '{f}' for '{cmd}'"))
        })
        .collect()
}

fn parse_index(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "invalid index '{s}': expected a positive number as shown by 'motder list'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Cli, String> {
        parse(args.iter().map(OsString::from), |_| None)
    }

    #[test]
    fn default_is_list() {
        let cli = p(&[]).unwrap();
        assert_eq!(cli.command, Command::List { json: false });
        assert_eq!(cli.state_dir, PathBuf::from(DEFAULT_STATE_DIR));
        assert_eq!(cli.motd_dir, PathBuf::from(DEFAULT_MOTD_DIR));
        assert_eq!(
            p(&["--json"]).unwrap().command,
            Command::List { json: true }
        );
    }

    #[test]
    fn add_collects_words_and_dash() {
        let cli = p(&["add", "hello", "world"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Add {
                words: vec!["hello".into(), "world".into()]
            }
        );
        let cli = p(&["add", "-"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Add {
                words: vec!["-".into()]
            }
        );
        assert!(p(&["add", "-x"]).is_err());
        let cli = p(&["add", "--", "-x", "--help"]).unwrap();
        assert_eq!(
            cli.command,
            Command::Add {
                words: vec!["-x".into(), "--help".into()]
            }
        );
    }

    #[test]
    fn dismiss_indices() {
        assert_eq!(
            p(&["dismiss"]).unwrap().command,
            Command::Dismiss { indices: vec![] }
        );
        assert_eq!(
            p(&["rm", "2", "5"]).unwrap().command,
            Command::Dismiss {
                indices: vec![2, 5]
            }
        );
        assert!(p(&["dismiss", "0"]).is_err());
        assert!(p(&["dismiss", "x"]).is_err());
        assert!(p(&["dismiss", "-1"]).is_err());
    }

    #[test]
    fn global_options_anywhere() {
        let cli = p(&["--state-dir", "/s", "ls", "--motd-dir=/m"]).unwrap();
        assert_eq!(cli.state_dir, PathBuf::from("/s"));
        assert_eq!(cli.motd_dir, PathBuf::from("/m"));
        assert!(p(&["--state-dir"]).is_err());
        assert!(p(&["--state-dir="]).is_err());
    }

    #[test]
    fn env_fallback_and_override() {
        let env = |k: &str| (k == "MOTDER_STATE_DIR").then(|| OsString::from("/env"));
        let cli = parse(Vec::<OsString>::new(), env).unwrap();
        assert_eq!(cli.state_dir, PathBuf::from("/env"));
        let cli = parse(vec![OsString::from("--state-dir=/flag")], env).unwrap();
        assert_eq!(cli.state_dir, PathBuf::from("/flag"));
    }

    #[test]
    fn misc_commands() {
        assert_eq!(p(&["--help"]).unwrap().command, Command::Help);
        assert_eq!(p(&["add", "-h"]).unwrap().command, Command::Help);
        assert_eq!(p(&["-V"]).unwrap().command, Command::Version);
        assert_eq!(
            p(&["uninstall", "--purge"]).unwrap().command,
            Command::Uninstall { purge: true }
        );
        assert!(p(&["clear", "extra"]).is_err());
        assert!(p(&["list", "--bogus"]).is_err());
        assert!(p(&["frobnicate"]).is_err());
    }
}
