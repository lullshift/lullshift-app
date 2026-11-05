//! Developer client: sends one agent event to a running Lullshift.
//!
//! ```text
//! cargo run -p agent-ipc --example send_event -- <app-data-dir> <event.json | ->
//! ```
//!
//! `<app-data-dir>` is the directory that holds `ipc/endpoint.json` (see docs/DEVELOPMENT.md).
//! Prints the reply line; exits 1 on rejection, 2 if the app isn't reachable.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(src)) = (args.next(), args.next()) else {
        eprintln!("usage: send_event <app-data-dir> <event.json | ->");
        std::process::exit(64);
    };
    let json = if src == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map(|_| s)
    } else {
        std::fs::read_to_string(&src)
    };
    let Ok(json) = json else {
        eprintln!("cannot read event input");
        std::process::exit(64);
    };
    match agent_ipc::send_event(&PathBuf::from(dir), json.trim(), Duration::from_secs(3)).await {
        Ok(reply) => {
            println!("{}", serde_json::to_string(&reply).unwrap_or_default());
            std::process::exit(if reply.ok { 0 } else { 1 });
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
