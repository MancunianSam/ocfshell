use assert_cmd::Command;
use predicate::str::contains;
use predicates::prelude::*;
use std::error::Error;
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

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

fn generate_object(ocfl_root: &PathBuf) {
    let inventory = "{\"id\":\"id\",\"head\":\"v1\",\"manifest\":{\"abc\":[\"a/b/c/d/e\"]},\"versions\":{\"v1\":{\"created\":\"\",\"state\":{\"abc\":[\"a/b/c\"]}}}}";
    let object_root = ocfl_root.join(PathBuf::from(
        "a56/145/270/a56145270ce6b3bebd1dd012b73948677dd618d496488bc608a3cb43ce3547dd",
    ));
    let file_root = object_root.join(PathBuf::from("a/b/c/d"));
    create_dir_all(&file_root).unwrap();
    File::create(&object_root.join("0=ocfl_object_1.1")).unwrap();
    let mut inventory_file = File::create(&object_root.join("inventory.json")).unwrap();
    inventory_file.write(inventory.as_bytes()).unwrap();
    let mut stored_file = File::create(&file_root.join("e")).unwrap();
    stored_file.write("test".as_bytes()).unwrap();
}

#[test]
fn ocfl_shell_outputs_expected_commands() {
    let td = tempdir().unwrap();
    let ocfl_root = &td.path().to_path_buf();

    generate_storage_json(&ocfl_root).unwrap();
    generate_object(&ocfl_root);
    File::create(&ocfl_root.join("0=ocfl_1.1")).unwrap();

    let mut cmd = Command::cargo_bin("ocfshell").unwrap();

    cmd.current_dir(td.path());

    let input = vec![
        "pwd",
        "ls",
        "cd id",
        "ls",
        "ls a/b/c",
        "pwd",
        "cat a/b/c",
        "exit",
    ]
    .join("\n");
    cmd.write_stdin(input);

    cmd.assert()
        .success()
        .stdout(contains(td.path().display().to_string()))
        .stdout(contains("id"))
        .stdout(contains("a/b/c"))
        .stdout(contains("4	a/b/c/d/e"))
        .stdout(contains("Object ID id"))
        .stdout(contains(format!(
            "{}/a56/145/270/a56145270ce6b3bebd1dd012b73948677dd618d496488bc608a3cb43ce3547dd",
            td.path().display()
        )))
        .stdout(contains("test"));
}
