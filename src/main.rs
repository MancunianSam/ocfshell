use ocfshell::shell::{Shell, ShellControl};
use std::process::exit;
use std::{
    error::Error,
    io::{Write, stdout},
};

fn main() -> std::process::ExitCode {
    match run() {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e);
            std::process::ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut shell = Shell::new()?;

    print_welcome()?;

    loop {
        match shell.read_and_process()? {
            ShellControl::Continue => (),
            ShellControl::Exit => exit(0),
        }
    }
}

fn print_welcome() -> Result<(), Box<dyn Error>> {
    println!(
        r"
   ___   ___ ___ ___ _        _ _
  / _ \ / __| __/ __| |_  ___| | |
 | (_) | (__| _|\__ \ ' \/ -_) | |
  \___/ \___|_| |___/_||_\___|_|_|
"
    );
    println!(" Welcome to OCFShell! Type 'exit' to quit.\n");

    stdout().flush()?;
    Ok(())
}
