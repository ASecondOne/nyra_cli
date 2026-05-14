# NyraCLI Task List

## Core Shell Features

* [x] Command execution
* [x] Argument parsing (shell-words)
* [x] Built-in `cd`
* [x] Built-in `exit`
* [x] Built-in `openhere`
* [x] Ctrl+C kills running process (not shell)
* [x] PATH command lookup
* [x] Output redirection (`>`, `>>`)

---

## Environment Variables

* [x] Basic `set KEY=VALUE`
* [x] `$VAR` expansion improvements (inside strings)
* [x] Support `$VAR/suffix`
* [x] Support `${VAR}` syntax

---

## Git Integration

* [x] Detect git repo
* [x] Show current branch
* [x] Show untracked files (`+`)
* [x] Show modified files (`*`)
* [x] Show pushable commits (`↑`)
* [x] Show pullable commits (`↓`)
* [ ] Optimize git calls (avoid running multiple git commands every loop)

---

## Autocomplete

* [x] Basic TAB completion
* [x] `cd` folder completion
* [x] Path-aware completion (`~/`, `/`, nested folders)
* [x] Clean display (no full path spam)
* [x] Command name completion
* [ ] File completion (not just folders)
* [ ] Smarter matching (fuzzy / partial)

---

## UX / Prompt

* [x] Colored prompt
* [x] Right-side exit code
* [x] Git branch in prompt
* [ ] Better spacing / alignment polish
* [ ] Configurable prompt
* [ ] Optional icons / symbols toggle
* [ ] Color key words

---

## Parsing Improvements

* [x] Basic parsing
* [x] Quote-aware parsing
* [x] Pipe parsing (`|`)
* [x] Multiple pipes (`cmd1 | cmd2 | cmd3`)
* [ ] Input redirection (`<`)
* [ ] Command chaining (`&&`, `||`, `;`)

---

## History

* [ ] Persistent history file
* [ ] Ctrl+R reverse search
* [ ] Fancy history UI (fzf-style maybe 👀)

---

## Built-in Commands

* [x] `clear` builtin
* [x] `set` list all variables
* [x] `unset` variables
* [x] `which` command lookup
* [ ] `alias` support

---

## Stability / Safety

* [ ] Error handling cleanup
* [ ] Prevent crashes on invalid input
* [ ] Handle edge cases (empty args, missing files)
* [ ] Fallback shell if something breaks (Optional)

---

## Future / Advanced

* [ ] Job control
* [ ] Subcommands / plugins
* [ ] Config file (~/.nyracli)
* [ ] Replace default shell (long-term goal :3)