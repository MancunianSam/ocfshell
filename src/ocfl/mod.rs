mod api;

use crate::interrupt::{Interrupt, interrupted};
pub(crate) use crate::ocfl::api::OcflApi;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageConfig {
    _digest_algorithm: String,
    pub tuple_size: usize,
    pub number_of_tuples: usize,
    _short_object_root: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Inventory {
    pub id: String,
    pub head: String,
    pub manifest: BTreeMap<String, Vec<String>>,
    pub versions: BTreeMap<String, Version>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Version {
    pub created: String,
    pub state: BTreeMap<String, Vec<String>>,
}

pub struct Ocfl {
    pub ocfl_root: PathBuf,
    pub storage_config: StorageConfig,
}

impl Ocfl {
    pub fn new(ocfl_root: PathBuf) -> Result<Self, Box<dyn Error>> {
        let storage_config =
            Ocfl::storage_config(&ocfl_root).expect("This is not an OCFL repository");
        Ok(Ocfl {
            ocfl_root,
            storage_config,
        })
    }

    pub fn list_objects(
        &mut self,
        interrupt: &Interrupt,
    ) -> Result<Vec<Inventory>, Box<dyn Error>> {
        let mut inventories = Vec::new();
        for entry in WalkDir::new(&self.ocfl_root)
            .min_depth(self.storage_config.number_of_tuples + 1)
            .max_depth(self.storage_config.number_of_tuples + 1)
        {
            if interrupted(&interrupt) {
                return Err("Interrupted".into());
            }
            let mut path = entry?.path().to_path_buf();
            if self.is_file_in_dir(&path, "0=ocfl_object_") {
                path.push("inventory.json");
                inventories.push(Ocfl::json_from_file::<Inventory>(PathBuf::from(path))?)
            }
        }
        Ok(inventories)
    }

    pub fn is_repository_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>> {
        Ok(self.is_file_in_dir(path, "0=ocfl_"))
    }

    pub fn is_object_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>> {
        Ok(self.is_file_in_dir(path, "0=ocfl_object_"))
    }

    fn is_file_in_dir(&self, dir: &PathBuf, prefix: &str) -> bool {
        match fs::read_dir(dir) {
            Ok(read) => read
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .find(|name| name.starts_with(prefix))
                .is_some(),
            Err(_) => false,
        }
    }

    pub fn json_from_file<T: DeserializeOwned>(path: PathBuf) -> Result<T, Box<dyn Error>> {
        let file = File::open(path)?;
        serde_json::from_reader::<File, T>(file).map_err(Into::into)
    }

    pub fn list_versions(&mut self, path: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
        if self.is_object_root(path)? {
            let inventory = self.load_inventory(path.to_owned())?;
            Ok(inventory
                .versions
                .keys()
                .map(|k| k.to_string())
                .collect::<Vec<_>>())
        } else {
            Ok(vec![String::from(
                "Version command needs to be inside an object",
            )])
        }
    }

    pub fn path_for_id(&mut self, id: &str) -> PathBuf {
        let checksum = hex::encode(Sha256::digest(id.as_bytes()));
        let mut path = PathBuf::new();
        for n in 0..self.storage_config.number_of_tuples {
            let start = n * self.storage_config.tuple_size;

            let b = &checksum[start..start + self.storage_config.tuple_size];
            path.push(b);
        }
        path.push(checksum);
        path
    }

    pub fn storage_config(ocfl_root: &PathBuf) -> Result<StorageConfig, Box<dyn Error>> {
        let layout = File::open(ocfl_root.join("ocfl_layout.json"))?;
        let layout_json = serde_json::from_reader::<File, serde_json::Value>(layout)?;
        let extension = layout_json["extension"]
            .as_str()
            .ok_or("0004-hashed-n-tuple-storage-layout")?;
        let config_path = PathBuf::from(format!("extensions/{extension}/config.json").as_str());
        let storage_config = Ocfl::json_from_file(ocfl_root.join(&config_path))?;
        Ok(storage_config)
    }

    pub fn path_for_logical_path(
        &mut self,
        arg_path: &str,
        inventory: &Inventory,
        version: &Version,
    ) -> Option<String> {
        let maybe_path = version
            .state
            .iter()
            .find_map(|(checksum, paths)| {
                if paths.contains(&arg_path.to_string()) {
                    Some(checksum)
                } else {
                    None
                }
            })
            .and_then(|checksum| {
                inventory
                    .manifest
                    .get(checksum)
                    .and_then(|paths| paths.first())
            });
        maybe_path.map(|path| path.to_string())
    }

    pub fn head_version(inventory: &Inventory) -> Version {
        let version = inventory.versions.get(&inventory.head).expect(&format!(
            "Version {} missing from inventory json for id {}",
            inventory.head, inventory.id
        ));
        version.clone()
    }

    pub fn load_inventory(
        &mut self,
        mut current_dir: PathBuf,
    ) -> Result<Inventory, Box<dyn Error>> {
        current_dir.push("inventory.json");
        let inventory = Ocfl::json_from_file::<Inventory>(current_dir)?;
        Ok(inventory)
    }

    pub fn modify_if_ocfl_path(
        &mut self,
        path: &PathBuf,
        args: Vec<&str>,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        let mut cmd_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        if !args.is_empty() && self.is_object_root(path)? {
            let inventory: Inventory = self.load_inventory(path.to_owned())?;
            let version: Version = Ocfl::head_version(&inventory);
            let maybe_logical_path: Option<String> =
                self.path_for_logical_path(args.last().unwrap(), &inventory, &version);
            maybe_logical_path.map(|logical_path| {
                if path.join(PathBuf::from(&logical_path)).exists() {
                    cmd_args.pop();
                    cmd_args.push(logical_path);
                }
            });
        };
        Ok(cmd_args)
    }
}

impl OcflApi for Ocfl {
    fn ocfl_root(&self) -> &PathBuf {
        &self.ocfl_root
    }

    fn storage_config(&self) -> &StorageConfig {
        &self.storage_config
    }

    fn is_repository_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>> {
        Ocfl::is_repository_root(self, path)
    }

    fn is_object_root(&self, path: &PathBuf) -> Result<bool, Box<dyn Error>> {
        Ocfl::is_object_root(self, path)
    }

    fn list_versions(&mut self, path: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
        Ocfl::list_versions(self, path)
    }

    fn list_objects(&mut self, interrupt: &Interrupt) -> Result<Vec<Inventory>, Box<dyn Error>> {
        Ocfl::list_objects(self, interrupt)
    }

    fn load_inventory(&mut self, dir: PathBuf) -> Result<Inventory, Box<dyn Error>> {
        Ocfl::load_inventory(self, dir)
    }

    fn path_for_id(&mut self, id: &str) -> PathBuf {
        Ocfl::path_for_id(self, id)
    }

    fn modify_if_ocfl_path(
        &mut self,
        path: &PathBuf,
        args: Vec<&str>,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        self.modify_if_ocfl_path(path, args)
    }
}
#[cfg(test)]
mod test {
    use crate::interrupt;
    use crate::ocfl::{Inventory, Ocfl, Version};
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::fs::{File, create_dir_all, remove_file};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::{TempDir, tempdir};

    fn generate_storage_json(ocfl_root: &PathBuf) -> Result<(), Box<dyn Error>> {
        let mut file = File::create(ocfl_root.join(PathBuf::from("ocfl_layout.json")))?;
        let layout_json = "{\"extension\":\"0004-hashed-n-tuple-storage-layout\"}";
        file.write(layout_json.as_bytes())?;
        let config_json = "{\"extensionName\":\"0004-hashed-n-tuple-storage-layout\",\"digestAlgorithm\":\"sha256\",\"tupleSize\":3,\"numberOfTuples\":3,\"shortObjectRoot\":false}";
        let config_path = ocfl_root.join(PathBuf::from(
            "extensions/0004-hashed-n-tuple-storage-layout",
        ));
        create_dir_all(&config_path)?;
        let mut config_file = File::create(config_path.join("config.json"))?;
        config_file.write(config_json.as_bytes())?;
        Ok(())
    }

    fn create_inventory_file(
        id: &str,
        head: &str,
        state: BTreeMap<String, Vec<String>>,
        manifest: BTreeMap<String, Vec<String>>,
        path: &PathBuf,
    ) {
        let version = Version {
            created: "".to_string(),
            state,
        };
        let versions = BTreeMap::from([(String::from("v1"), version)]);
        let inventory = Inventory {
            id: id.to_string(),
            head: head.to_string(),
            manifest,
            versions,
        };
        let json_string = serde_json::to_string(&inventory).unwrap();
        let inventory_path = path.join("inventory.json");
        let mut inventory_file = File::create(inventory_path).unwrap();
        inventory_file.write(json_string.as_bytes()).unwrap();
    }

    fn create_empty_object(ocfl_root: &PathBuf, name: &str) -> PathBuf {
        let root_object_path = ocfl_root.join(PathBuf::from(name));
        File::create(&root_object_path).unwrap();
        root_object_path
    }

    fn create_ocfl(tmp_dir: &TempDir) -> (PathBuf, Ocfl) {
        let ocfl_root = tmp_dir.path().to_path_buf();
        generate_storage_json(&ocfl_root).unwrap();
        let ocfl = Ocfl::new(ocfl_root.clone()).unwrap();
        (ocfl_root, ocfl)
    }

    #[test]
    fn test_is_repository_root() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, ocfl) = create_ocfl(&tmp_dir);

        let root_object_path = create_empty_object(&ocfl_root, "0=ocfl_1.1");

        assert!(ocfl.is_repository_root(&ocfl_root).unwrap());

        remove_file(&root_object_path).unwrap();

        assert!(!ocfl.is_repository_root(&ocfl_root).unwrap());
    }

    #[test]
    fn test_is_object_root() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, ocfl) = create_ocfl(&tmp_dir);

        let root_object_path = create_empty_object(&ocfl_root, "0=ocfl_object_1.1");
        assert!(ocfl.is_object_root(&ocfl_root).unwrap());

        remove_file(&root_object_path).unwrap();

        assert!(!ocfl.is_object_root(&ocfl_root).unwrap());
    }

    #[test]
    fn test_list_versions() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        let state = BTreeMap::from([(String::from("abc"), vec![String::from("a/b/c")])]);
        create_inventory_file("id", "v1", state, BTreeMap::default(), &ocfl_root);

        let versions_not_in_root = ocfl.list_versions(&ocfl_root).unwrap();
        assert_eq!(
            versions_not_in_root,
            vec!["Version command needs to be inside an object"]
        );

        create_empty_object(&ocfl_root, "0=ocfl_object_1.1");

        let versions = ocfl.list_versions(&ocfl_root).unwrap();
        assert_eq!(versions, vec!["v1"])
    }

    #[test]
    fn test_list_objects_with_no_objects() {
        let tmp_dir = tempdir().unwrap();
        let (_ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);
        let interrupt = interrupt::install_sigint_flag();
        let empty_objects = ocfl.list_objects(&interrupt).unwrap();

        assert!(empty_objects.is_empty())
    }

    #[test]
    fn test_list_objects_with_objects() {
        fn create_object_dirs(ocfl_root: &PathBuf, path: &str, id: &str) {
            let object_path = ocfl_root.join(PathBuf::from(path));
            create_dir_all(&object_path).unwrap();
            create_inventory_file(
                id,
                "v1",
                BTreeMap::default(),
                BTreeMap::default(),
                &object_path,
            );
            create_empty_object(&object_path, "0=ocfl_object_1.1");
        }
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);
        let interrupt = interrupt::install_sigint_flag();

        create_object_dirs(&ocfl_root, "abc/def/ghi/jklmnopqr", "id1");
        create_object_dirs(&ocfl_root, "bcd/efg/hij/klmnopqrs", "id2");
        create_object_dirs(&ocfl_root, "cde/fgh/ijk/lmnopqrst", "id3");

        let mut objects: Vec<String> = ocfl
            .list_objects(&interrupt)
            .unwrap()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        let _ = &objects.sort();

        assert_eq!(objects, vec!["id1", "id2", "id3"])
    }

    #[test]
    fn test_load_inventory() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        create_inventory_file(
            "id",
            "v1",
            BTreeMap::default(),
            BTreeMap::default(),
            &ocfl_root,
        );

        let inventory = ocfl.load_inventory(ocfl_root).unwrap();

        assert_eq!(inventory.id, "id");
        assert_eq!(inventory.head, "v1");
        assert!(inventory.versions["v1"].state.is_empty());
        assert!(inventory.manifest.is_empty());
    }

    #[test]
    fn test_path_for_id() {
        let tmp_dir = tempdir().unwrap();
        let (_ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        let path_for_id = ocfl.path_for_id("abc");

        assert_eq!(
            path_for_id.display().to_string(),
            "ba7/816/bf8/ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_modify_if_not_object_root() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        let args_not_object_root = ocfl.modify_if_ocfl_path(&ocfl_root, vec!["abc"]).unwrap();

        assert_eq!(args_not_object_root, vec!["abc"]);
    }

    #[test]
    fn test_modify_if_no_match() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        create_empty_object(&ocfl_root, "0=ocfl_object_1.1");
        let state = BTreeMap::from([(String::from("abc"), vec![String::from("a/b/c")])]);
        let manifest = BTreeMap::from([(String::from("abc"), vec![String::from("a/b/c/d/e")])]);
        create_inventory_file("id", "v1", state, manifest, &ocfl_root);

        let args = ocfl.modify_if_ocfl_path(&ocfl_root, vec!["a/b/c"]).unwrap();

        assert_eq!(args.first().unwrap(), "a/b/c")
    }

    #[test]
    fn test_modify_if_an_ocfl_path() {
        let tmp_dir = tempdir().unwrap();
        let (ocfl_root, mut ocfl) = create_ocfl(&tmp_dir);

        create_empty_object(&ocfl_root, "0=ocfl_object_1.1");
        let state = BTreeMap::from([(String::from("abc"), vec![String::from("a/b/c")])]);
        let manifest = BTreeMap::from([(String::from("abc"), vec![String::from("a/b/c/d/e")])]);
        create_inventory_file("id", "v1", state, manifest, &ocfl_root);
        let file_dir = &ocfl_root.join(PathBuf::from("a/b/c/d"));
        create_dir_all(file_dir).unwrap();
        create_empty_object(&file_dir, "e");

        let args = ocfl.modify_if_ocfl_path(&ocfl_root, vec!["a/b/c"]).unwrap();
        assert_eq!(args.first().unwrap(), "a/b/c/d/e")
    }
}
