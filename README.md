# symlist

Simple command-line tool to manage and sync a list of symbolic links. It reads a configuration file and allows you to create or remove symbolic links in bulk.

## Installation

You can build and install the tool using Cargo:

```bash
# Install latest versoin from crates.io
cargo install symlist

# Install directly from git
cargo install --git https://github.com/fazuh/symlist

# Or build directly from project root
cargo install --path .
```

Or download the pre-compiled binary at https://github.com/FAZuH/symlist/releases

## Configuration

By default, the tool looks for a `symlink.lst` file in the current working directory. The file should contain one entry per line, using the format `original_path:symlink_path`. Empty lines and lines starting with `#` are ignored.

Example `symlink.lst`:

```
# Create a symlink to dotfiles
/home/user/dotfiles/bashrc:/home/user/.bashrc
/home/user/dotfiles/vimrc:/home/user/.vimrc
```

## Usage

You can use the `symlink` or `unlink` commands to manage your links.

```bash
# Create all symlinks defined in the configuration
symlist symlink

# Remove all symlinks defined in the configuration:
symlist unlink

# To specify a custom configuration file, use the `--link-conf` or `-l` option:
symlist --link-conf /path/to/custom.lst symlink
```
