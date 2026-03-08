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
ward               # open last workspace, or ~/ward by default
ward <directory>   # open a specific directory as the workspace
ward import <file.md>  # import reminders from a markdown file
```

## Keybindings

### Sidebar (Lists & Notes panel)

| Key | Action |
|-----|--------|
| `j` / `k` or `↑` / `↓` | Navigate |
| `Shift+↑` / `Shift+↓` | Reorder items |
| `Enter` / `→` | Enter list / expand folder |
| `n` | New reminder list |
| `N` | New note |
| `f` | New folder/group |
| `g` | Move selected item into a folder/group |
| `e` | Rename list or note / open note in `$EDITOR` |
| `d` / `Delete` | Delete item |
| `u` | Undo |
| `x` | Export to `~/name.md` |

### Reminders panel

| Key | Action |
|-----|--------|
| `j` / `k` or `↑` / `↓` | Navigate |
| `n` | New reminder |
| `e` | Edit reminder |
| `Space` | Toggle done |
| `d` / `Delete` | Delete |
| `m` | Move to another list |
| `V` | Enter bulk-select mode |
| `s` | Cycle sort order |
| `h` | Toggle show completed |
| `/` | Search |

### Bulk-select mode (`V`)

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate |
| `Space` | Toggle selection |
| `Enter` | Mark all selected done |
| `d` | Delete all selected |
| `m` | Move all selected to another list |
| `Esc` | Cancel |

### Global

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch panels |
| `u` | Undo |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

## CLI commands

```sh
# List reminders
ward ls                          # all reminders
ward ls --list Work              # filter by list name
ward ls --today                  # due today
ward ls --overdue                # overdue only
ward ls --pending                # incomplete only

# Add a reminder without opening the TUI
ward add "Buy milk" --list Personal --due tomorrow --priority high --tags grocery

# Mark a reminder done by title
ward done "buy milk"

# Import from a markdown checklist
ward import tasks.md
```

## Data Format

Ward stores data in a directory of JSON files (reminder lists) and markdown files (notes). The default workspace is `~/ward`.

## Crates

| Crate | Description |
|-------|-------------|
| `ward-core` | Data model, persistence, and notification logic |
| `ward-tui` | Terminal UI and CLI entry point |
| `ward-daemon` | Background daemon binary |
