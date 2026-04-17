use std::{env, process};

fn usage() -> ! {
    eprintln!("Usage: crostini -- <command> [args...]");
    process::exit(1);
}

fn main() {
    let args = env::args().collect::<Vec<_>>();

    let sep = args
        .iter()
        .position(|a| a == "--")
        .unwrap_or_else(|| usage());
    let child_argv = &args[sep + 1..];
    if child_argv.is_empty() {
        usage();
    }

    process::exit(crostini::run(child_argv));
}
