//! motder — manage your own messages in Ubuntu's dynamic message of the day.
//!
//! Messages are stored in `/var/lib/motder` and shown on login through a
//! single hook script installed in `/etc/update-motd.d`.

mod app;
mod cli;
mod error;
mod hook;
mod render;
mod store;
mod time;

use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = match cli::parse(std::env::args_os().skip(1), |k| std::env::var_os(k)) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("motder: {msg}");
            eprintln!("Try 'motder --help' for more information.");
            return ExitCode::from(2);
        }
    };

    match app::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("motder: error: {err}");
            if err.is_permission_denied() {
                eprintln!("hint: changing the message of the day needs root; try again with sudo");
            }
            ExitCode::FAILURE
        }
    }
}
