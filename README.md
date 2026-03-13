# ocfshell

`ocfshell` is a small interactive shell for working with an [OCFL](https://ocfl.io/) repository on disk. It provides a handful of built-in commands (`ls`, `cd`, `pwd`, `versions`, `exit`) and can also run external commands, with basic pipeline support (`cmd1 | cmd2`).

> Status: this project is in active development and the command set / behavior may change.

## Features

- Interactive prompt powered by `rustyline`
- Built-in commands tailored to OCFL repositories/objects:
    - `ls` — list repository objects or object content paths (and basic file info in some cases)
    - `cd` — change directory (with OCFL-aware behavior)
    - `pwd` — show current directory (and OCFL object id when applicable)
    - `versions` — list OCFL versions
    - `exit` — exit the shell
- Run external commands (anything not matched as a builtin)
- Basic pipelines using ` | ` (space-pipe-space), e.g. `ls | head`

## Requirements

- Rust toolchain (stable) via <https://rustup.rs>
- A local OCFL repository on disk (for most commands)

## Install / Build

Clone and build:

```bash
cargo build
```

Run:

```bash
cargo run
```

To build a release binary:

```bash
cargo build --release
./target/release/ocfshell
```

## Usage

Start `ocfshell` from within (or somewhere near) an OCFL repository on disk:

```bash
ocfshell
```

### Built-in commands

#### `versions`
Lists available versions for the current OCFL object/repository context.

#### `pwd`
Prints the current working directory. If you are inside an OCFL object, it may also print the object identifier.

#### `cd [path|object-id]`
Change directory. When run at the OCFL repository root, `cd <object-id>` will attempt to change into that object’s directory.

#### `ls [path|object-id]`
Lists content depending on context:

- At the OCFL repository root:
    - `ls` prints object identifiers
    - `ls <object-id>` prints logical paths (from the object inventory)
- Inside an OCFL object:
    - `ls` prints logical paths (from the inventory head version state)
    - `ls <path>` may show basic file information (currently size + path)

### External commands and pipelines

Any non-builtin command is executed as an external process:

```bash
echo hello
```

You can pipe output between commands using **space-pipe-space** (`" | "`):

```bash
ls | head
ls | grep txt
```

## Development

### Run tests

Unit tests:

```bash
cargo test
```

Integration tests (if present under `tests/`):

```bash
cargo test --test cli -- --nocapture
```

### Notes on testing

- Unit tests generally inject a custom `Write` sink to capture output rather than writing to real stdout.
- When testing behavior that uses external processes or terminal/TTY behavior, prefer integration tests.

## History file

`ocfshell` stores command history in:

- `~/.ocfshell_history` (or `/tmp/.ocfshell_history` if a home directory cannot be determined)

## License

