Still under heavy development. When using it,evaluate at your own risk. However this program has something safer: preview mode, default to non-execution mode.

## Help

This project is an assistant for file synchronization with git, borg, rsync, etc.

### `dionysius --help`


```
Usage: dionysius [OPTIONS] [COMMAND]

Commands:
  conf  Lists all git repositories in a directory
  push  Push to various backup targets
  test  Test the function
  help  Print this message or the help of the given subcommand(s)

Options:
  -t, --threads <Thread Number>  Sets the maximum number of threads to use (Note: no any effect now!)
  -h, --help                     Print help
  -V, --version                  Print version
```

### `dionysius push --help`

```
Push to various backup targets

Usage: dionysius push [OPTIONS] [COMMAND]

Commands:
  git      Push to remote git repositories
  borg     Push to remote git repositories
  trigger  Let dionysius traverse directories and trigger backup tasks
  help     Print this message or the help of the given subcommand(s)

Options:
  -p, --preview            Preview mode - show what would be done
  -e, --execute            Enter execution mode, which performs real operations instead of print commands.
  -x, --exclude <PATTERN>  Exclude pattern to be added to the tasks
  -H, --search-hidden      Go into directories whose name begins with `.`
  -h, --help               Print help
```

### `dionysius conf --help`

```
Dump the config from give path by using Rust `Debug` trait.

Usage: dionysius conf --input <Input config file>

Options:
  -i, --input <Input config file>  
  -h, --help                       Print help
```


## Design Note

TODO:

[ ] possibly convert `exclude_list` as exact pattern, and merge with those pattern. This simplifies codes, but may lower performance due to pattern matching.

[ ] must support subfolder exclusion to skip some strange git dir, e.g. `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/1737975252057/checkpoints`. currently gitignore is not equivalent to that, as those repo may be even unable to open (causing panic).

[ ] if some local/remote branch points to a local/remote branch, do not regard it as counted into multiple branches check.

[ ] ignore `.local/share/Trash/files/dionysius.toml`

### toml

`require_sub`: push this dir means to force each subdirectory to be push-able. if not, gen warn.
