# motder

> [!NOTE]
> Implemented with assistance from Claude

Add, list and dismiss your own messages in Ubuntu's dynamic **message of the day**.

```console
$ sudo motder add 'Maintenance window: Saturday 02:00-04:00 UTC'
Installed hook /etc/update-motd.d/99-motder
Added message [1].

$ df -h /data | sudo motder add
Added message [2].

$ motder list
#  ADDED     MESSAGE
1  2h ago    Maintenance window: Saturday 02:00-04:00 UTC
2  just now  Filesystem      Size  Used Avail Use% Mounted on
             /dev/sdb1       1.8T  1.6T  180G  91% /data

$ sudo motder dismiss          # the most recent message
Dismissed [2]: Filesystem      Size  Used Avail Use% Mounted on …
```

At the next login, users see the messages after Ubuntu's own sections:

```text
 * Maintenance window: Saturday 02:00-04:00 UTC
```

## Install

```sh
cargo install motder
```

`motder` has no dependencies beyond the Rust standard library. Building it
requires Rust 1.89 or newer. A `.deb` can be built with
[`cargo deb`](https://crates.io/crates/cargo-deb); the package includes a man
page and bash completion.

## Usage

| Command | What it does |
| --- | --- |
| `motder add <MESSAGE>...` | Add a message. Words are joined with spaces. With no message, or `-`, it is read from stdin (multi-line is fine). |
| `motder list` / `ls` | List messages with their index. This is the default when no command is given. `--json` gives machine-readable output. |
| `motder dismiss [INDEX]...` / `rm` | Dismiss messages by index. **With no index, dismisses the last (most recent) one.** Several indices can be given at once. |
| `motder clear` | Dismiss all messages. |
| `motder show` | Print exactly what motder contributes to the login screen. |
| `motder install` | Install or repair the hook (done automatically by every change). |
| `motder uninstall [--purge]` | Remove the hook. Messages are kept unless `--purge` is given. |

Indices are positions in the current list (1 is the oldest), so they shift
after a dismissal. Run `motder list` to see the current numbering. A command
that names an invalid index fails without dismissing anything.

Changing messages requires root. Listing works for any user.

To start a message with `-`, put `--` first: `sudo motder add -- '-5% CPU headroom'`.

To preview the complete message of the day, run `sudo run-parts /etc/update-motd.d`.

## How it works

Ubuntu's `pam_motd` runs every executable in `/etc/update-motd.d` at login
and shows their combined output. motder:

- keeps its messages in `/var/lib/motder/messages`;
- renders the text to show in `/var/lib/motder/motd` whenever messages change;
- installs one hook, `/etc/update-motd.d/99-motder`, a three-line shell script
  that prints that file. The `99-` prefix places the messages last, just above
  the shell prompt.

The hook never runs `motder` itself, so logins are unaffected if the binary is
removed. Updates are atomic and serialized with a lock, so concurrent
invocations are safe. motder never overwrites or deletes a `99-motder` file it
did not create.

### Configuration

| Option | Environment variable | Default |
| --- | --- | --- |
| `--state-dir <DIR>` | `MOTDER_STATE_DIR` | `/var/lib/motder` |
| `--motd-dir <DIR>` | `MOTDER_MOTD_DIR` | `/etc/update-motd.d` |

Command-line options take precedence over environment variables. The hook
records the state directory that was in use when it was written.

## Uninstalling

```sh
sudo motder uninstall --purge   # remove the hook and all messages
cargo uninstall motder
```

## License

Licensed under

- [Apache License, Version 2.0](LICENSE)
