use crate::interrupt::Interrupt;
use crate::ocfl::{Inventory, StorageConfig};
use std::{error::Error, path::PathBuf};

pub trait OcflApi {
    fn ocfl_root(&self) -> &PathBuf;
    fn storage_config(&self) -> &StorageConfig;

    fn is_repository_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>>;
    fn is_object_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>>;

    fn list_versions(&mut self, path: &PathBuf) -> Result<Vec<String>, Box<dyn Error>>;
    fn list_objects(&mut self, interrupt: &Interrupt) -> Result<Vec<Inventory>, Box<dyn Error>>;

    fn load_inventory(&mut self, dir: PathBuf) -> Result<Inventory, Box<dyn Error>>;

    fn path_for_id(&mut self, id: &str) -> PathBuf;

    fn modify_if_ocfl_path(
        &mut self,
        path: &PathBuf,
        args: Vec<&str>,
    ) -> Result<Vec<String>, Box<dyn Error>>;
}
