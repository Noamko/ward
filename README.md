# ward

A terminal-based reminder and notes manager built with Rust and [ratatui](https://github.com/ratatui-org/ratatui).

## Features

- Reminder lists with due dates, priorities, tags, subtasks, and recurrence
- Markdown notes with `$EDITOR` support
- Folders for organizing lists and notes
- Background daemon for desktop notifications
- Import reminders from markdown files
- Search, sort, undo, and move reminders between lists

## Installation

```sh
./install.sh
```

This builds the release binary, installs it to `~/.cargo/bin/ward`, sets up a background notification daemon (systemd on Linux, launchd on macOS), and installs fish completions if available.

## Usage

```sh
ward               # open last workspace, or ~/rmdr by default
ward <directory>   # open a specific directory as the workspace
ward import <file.md>  # import reminders from a markdown file
```

## Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch focus between panels |
| `j` / `k` or arrow keys | Navigate |
| `n` | New reminder (in list) / new list (in sidebar) |
| `N` | New note |
| `f` | New folder |
| `e` | Edit selected item / open note in `$EDITOR` |
| `d` / `Delete` | Delete selected item |
| `Space` | Toggle reminder done |
| `m` | Move reminder to another list |
| `s` | Cycle sort order |
| `h` | Toggle show completed reminders |
| `u` | Undo |
| `x` | Export current list or note |
| `/` | Search |
| `Shift+Up/Down` | Reorder sidebar items |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

## Data Format

Ward stores data in a directory of JSON files (reminder lists) and markdown files (notes). The default workspace is `~/rmdr`.

## Crates

| Crate | Description |
|-------|-------------|
| `ward-core` | Data model, persistence, and notification logic |
| `ward-tui` | Terminal UI and CLI entry point |
| `ward-daemon` | Background daemon binary |
