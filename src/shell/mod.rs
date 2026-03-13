mod control;

use crate::interrupt;
use crate::ocfl::Ocfl;
use crate::process::Process;
pub use crate::shell::control::ShellControl;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{DefaultEditor, Editor};
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
use std::{env, io};

pub struct Shell {
    rl: Editor<(), DefaultHistory>,
    history_path: PathBuf,
    process: Process<Ocfl>,
}
impl Shell {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut rl = DefaultEditor::new()?;
        let history_path = Self::load_history(&mut rl)?;
        let interrupt = interrupt::install_sigint_flag();
        let ocfl = Ocfl::new(env::current_dir()?)?;
        let process = Process {
            ocfl,
            interrupt,
            out: Box::new(io::stdout()),
        };
        Ok(Self {
            rl,
            history_path,
            process,
        })
    }

    fn load_history(rl: &mut Editor<(), DefaultHistory>) -> Result<PathBuf, Box<dyn Error>> {
        let mut history_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        history_path.push(".ocfshell_history");

        match rl.load_history(&history_path) {
            Ok(_) => {}
            Err(ReadlineError::Io(_)) => {
                File::create(&history_path)?;
            }
            Err(err) => {
                eprintln!("ocfshell: Error loading history: {}", err);
            }
        }
        Ok(history_path)
    }

    pub fn read_and_process(&mut self) -> Result<ShellControl, Box<dyn Error>> {
        match self.rl.readline("> ") {
            Ok(line) => {
                let input = line.trim();
                self.rl.add_history_entry(input)?;
                match self.process.process(input) {
                    Ok(control) => Ok(control),
                    Err(e) => {
                        eprintln!("{}", e);
                        Ok(ShellControl::Continue)
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("Interrupted");
                self.exit()?;
                Ok(ShellControl::Exit)
            }
            Err(e) => {
                eprintln!("Error {:?}", e);
                Ok(ShellControl::Continue)
            }
        }
    }

    pub fn exit(&mut self) -> Result<(), Box<dyn Error>> {
        Ok(self.rl.save_history(&self.history_path)?)
    }
}
