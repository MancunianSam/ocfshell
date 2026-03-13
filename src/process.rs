use crate::interrupt::{Interrupt, clear_interrupt};
use crate::ocfl::OcflApi;
use crate::ocfl::*;
use crate::shell::ShellControl;
use std::env;
use std::env::current_dir;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::io::{PipeReader, stdout};
use std::option::Option;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub struct Process<O: OcflApi> {
    pub ocfl: O,
    pub interrupt: Interrupt,
    pub out: Box<dyn Write>,
}

impl<O: OcflApi> Process<O> {
    pub fn new(ocfl: O, interrupt: Interrupt) -> Result<Self, Box<dyn Error>> {
        Ok(Process {
            ocfl,
            interrupt,
            out: Box::new(stdout()),
        })
    }

    pub fn process(&mut self, input: &str) -> Result<ShellControl, Box<dyn Error>> {
        clear_interrupt(&self.interrupt);
        if input.is_empty() {
            return Ok(ShellControl::Continue);
        }

        let mut commands = input.trim().split(" | ").peekable();

        let mut prev_stdout: Option<Stdio> = None;
        let mut children: Vec<Child> = Vec::new();

        while let Some(command) = commands.next() {
            let current_dir = current_dir()?;
            let mut parts = command.split_whitespace();
            let Some(command) = parts.next() else {
                continue;
            };
            let args: Vec<&str> = parts.collect();
            if command == "exit" {
                return Ok(ShellControl::Exit);
            }
            let output: Option<PipeReader> = match command {
                "versions" => self.run_versions()?,
                "pwd" => self.run_pwd()?,
                "cd" => self.run_cd(&args)?,
                "ls" => self.run_ls(args)?,
                command => {
                    let cmd_args = self.ocfl.modify_if_ocfl_path(&current_dir, args)?;

                    let stdin = match prev_stdout.take() {
                        Some(output) => Stdio::from(output),
                        None => Stdio::inherit(),
                    };

                    let stdout = if commands.peek().is_some() {
                        Stdio::piped()
                    } else {
                        Stdio::inherit()
                    };

                    let mut child = Command::new(command)
                        .args(cmd_args)
                        .stdin(stdin)
                        .stdout(stdout)
                        .spawn()?;

                    let out = child.stdout.take();
                    prev_stdout = out.map(Stdio::from);
                    children.push(child);
                    None
                }
            };
            if let Some(reader) = output {
                if commands.peek().is_some() {
                    prev_stdout = Some(Stdio::from(reader));
                } else {
                    std::io::copy(&mut &reader, &mut self.out)?;
                }
            } else {
                prev_stdout = None;
            }
        }
        for mut child in children {
            let _ = child.wait();
        }
        Ok(ShellControl::Continue)
    }

    fn run_versions(&mut self) -> Result<Option<PipeReader>, Box<dyn Error>> {
        let (reader, mut writer) = std::io::pipe()?;
        let versions = self.ocfl.list_versions(&current_dir()?)?;
        for version in versions {
            writeln!(writer, "{}", version)?
        }
        Ok(Some(reader))
    }

    fn run_ls(&mut self, args: Vec<&str>) -> Result<Option<PipeReader>, Box<dyn Error>> {
        let current_dir = current_dir()?;
        let paths: Vec<String> = if self.ocfl.is_object_root(&current_dir)? {
            let inventory = self.ocfl.load_inventory(env::current_dir()?)?;
            let version = Ocfl::head_version(&inventory);
            if args.is_empty() {
                version
                    .state
                    .iter()
                    .flat_map(|(_k, v)| v.iter().cloned())
                    .collect()
            } else {
                let cmd_args = self.ocfl.modify_if_ocfl_path(&current_dir, args)?;
                let file_name = cmd_args.last().unwrap();
                let file = File::open(file_name)?;
                let metadata = file.metadata()?;
                let size = metadata.size();
                vec![format!("{}\t{}", size, file_name)]
            }
        } else if self.ocfl.is_repository_root(&current_dir)? {
            if args.is_empty() || args.last().is_some_and(|arg| arg.starts_with("-")) {
                self.ocfl
                    .list_objects(&self.interrupt)?
                    .iter()
                    .map(|i| i.id.clone())
                    .collect()
            } else {
                let arg_path = args.last().unwrap();
                let object_path = self.ocfl.path_for_id(arg_path);
                let inventory = self.ocfl.load_inventory(object_path)?;
                let version = Ocfl::head_version(&inventory);
                version
                    .state
                    .iter()
                    .flat_map(|(_k, v)| v.iter().cloned())
                    .collect()
            }
        } else {
            vec![String::from("Nothing to do")]
        };

        let (reader, mut writer) = std::io::pipe()?;

        for path in paths {
            writeln!(writer, "{}", path)?
        }
        drop(writer);
        Ok(Some(reader))
    }

    fn run_cd(&mut self, args: &Vec<&str>) -> Result<Option<PipeReader>, Box<dyn Error>> {
        let chdir_or_current = || {
            env::set_current_dir(args.last().map(PathBuf::from).unwrap_or(current_dir()?))?;
            Ok::<(), Box<dyn Error>>(())
        };

        if self.ocfl.is_repository_root(&current_dir()?)? {
            let new_dir = args
                .first()
                .map(|id| self.ocfl.path_for_id(id))
                .unwrap_or(self.ocfl.ocfl_root().clone());
            if new_dir.exists() {
                let root = Path::new(new_dir.as_path());
                env::set_current_dir(root)?
            } else {
                chdir_or_current()?
            }
        } else if !args.is_empty() && self.ocfl.is_object_root(&current_dir()?)? {
            let arg_path = args.last().unwrap();
            if arg_path == &".." {
                env::set_current_dir(&self.ocfl.ocfl_root())?
            } else {
                chdir_or_current()?
            }
        } else {
            chdir_or_current()?
        }
        Ok(None)
    }

    fn run_pwd(&mut self) -> Result<Option<PipeReader>, Box<dyn Error>> {
        let (reader, mut writer) = std::io::pipe()?;
        let current_dir = current_dir()?;
        if self.ocfl.is_object_root(&env::current_dir()?)? {
            let inventory = self.ocfl.load_inventory(current_dir)?;
            writeln!(writer, "Object ID {}", inventory.id)?
        }
        writeln!(writer, "{}", env::current_dir()?.display().to_string())?;
        Ok(Some(reader))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interrupt;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::fs::create_dir_all;
    use std::io;
    use std::rc::Rc;
    use tempfile::tempdir;

    #[derive(Clone, Default)]
    pub struct CaptureWriter(pub Rc<RefCell<Vec<u8>>>);

    impl Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct TestOcfl {
        ocfl_root: PathBuf,
        is_object_root: bool,
        is_repository_root: bool,
        inventory: Inventory,
    }

    impl OcflApi for TestOcfl {
        fn ocfl_root(&self) -> &PathBuf {
            &self.ocfl_root
        }

        fn storage_config(&self) -> &StorageConfig {
            todo!()
        }

        fn is_repository_root(&self, _path: &PathBuf) -> Result<bool, Box<dyn Error>> {
            Ok(self.is_repository_root)
        }

        fn is_object_root(&self, _path: &PathBuf) -> Result<bool, Box<dyn Error>> {
            Ok(self.is_object_root)
        }

        fn list_versions(&mut self, _path: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
            todo!()
        }

        fn list_objects(
            &mut self,
            _interrupt: &Interrupt,
        ) -> Result<Vec<Inventory>, Box<dyn Error>> {
            Ok(vec![self.inventory.clone()])
        }

        fn load_inventory(&mut self, dir: PathBuf) -> Result<Inventory, Box<dyn Error>> {
            Ok(Inventory {
                id: dir.display().to_string(),
                head: self.inventory.head.clone(),
                manifest: self.inventory.manifest.clone(),
                versions: self.inventory.versions.clone(),
            })
        }

        fn path_for_id(&mut self, id: &str) -> PathBuf {
            if id == "file" {
                self.ocfl_root.join(PathBuf::from("a/b/c/abc-123/v1/file"))
            } else {
                self.ocfl_root.join(PathBuf::from("a/b/c/abc-123"))
            }
        }

        fn modify_if_ocfl_path(
            &mut self,
            _path: &PathBuf,
            _args: Vec<&str>,
        ) -> Result<Vec<String>, Box<dyn Error>> {
            Ok(vec![format!(
                "{}/a/b/c/abc-123/v1/file",
                self.ocfl_root.display()
            )])
        }
    }

    fn run_process_tests(
        input: &str,
        expected_output: &str,
        is_repository_root: bool,
        expected_response: ShellControl,
    ) -> String {
        let ocfl_root = tempdir().unwrap().keep();
        let object_root = ocfl_root.join(PathBuf::from("a/b/c/abc-123"));
        let file_path = object_root.join(PathBuf::from("v1"));
        create_dir_all(&file_path).unwrap();
        File::create(file_path.join("file")).unwrap();
        let inventory: Inventory = create_inventory("v1", vec![String::from("/test/path")]);
        let ocfl = TestOcfl {
            ocfl_root,
            is_object_root: !is_repository_root,
            is_repository_root,
            inventory,
        };
        let interrupt = interrupt::install_sigint_flag();
        let cap = CaptureWriter::default();
        let shared = cap.0.clone();
        let mut process = Process {
            ocfl,
            interrupt,
            out: Box::new(cap),
        };
        let res = process.process(input).unwrap();
        let output = String::from_utf8(shared.borrow().clone()).unwrap();
        assert_eq!(res, expected_response);
        assert!(output.trim().contains(expected_output));
        let new_dir = current_dir().unwrap();
        new_dir.as_path().display().to_string()
    }

    fn create_inventory(version_num: &str, paths: Vec<String>) -> Inventory {
        let version = Version {
            state: BTreeMap::from([(String::from(""), paths)]),
            created: String::from(""),
        };
        Inventory {
            id: "object-id".to_string(),
            head: version_num.to_string(),
            manifest: Default::default(),
            versions: BTreeMap::from([(String::from(version_num), version)]),
        }
    }

    #[test]
    fn test_ls_in_root_directory() {
        run_process_tests("ls", "object-id", true, ShellControl::Continue);
    }

    #[test]
    fn test_ls_in_root_directory_with_path_arg() {
        run_process_tests("ls abc-123", "/test/path", true, ShellControl::Continue);
    }

    #[test]
    fn test_ls_in_object_directory() {
        run_process_tests("ls", "/test/path", false, ShellControl::Continue);
    }

    #[test]
    fn test_ls_in_object_directory_with_path_arg() {
        run_process_tests(
            "ls /test/path",
            "a/b/c/abc-123/v1/file",
            false,
            ShellControl::Continue,
        );
    }

    #[test]
    fn run_pwd_in_object_directory() {
        run_process_tests("pwd", "Object ID", false, ShellControl::Continue);
    }

    #[test]
    fn run_exit() {
        run_process_tests("exit", "", true, ShellControl::Exit);
    }
}
