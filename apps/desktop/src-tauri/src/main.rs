// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "hook") {
        // Hook mode (ADR-010): run by an agent's hook, never starts the app. Zero-effect
        // contract: always exit 0 and write nothing to stdout or stderr, even on panic.
        // The time is taken first so separate hook processes keep the agent's order.
        let started = time::OffsetDateTime::now_utc();
        std::panic::set_hook(Box::new(|_| std::process::exit(0)));
        // Arguments that aren't valid Unicode can't be ours: drop the event.
        let args: Option<Vec<String>> = args.map(|a| a.into_string().ok()).collect();
        if let Some(args) = args {
            let _ = agent_adapters::forward::run(&args, std::io::stdin(), started);
        }
        std::process::exit(0);
    }
    lullshift_desktop_lib::run()
}
