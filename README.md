# QBasic Studio

QBasic Studio is a Rust terminal IDE and interpreter for a practical,
QBasic-inspired BASIC dialect. It includes an editor, a separate run window,
interactive input, file I/O, simple graphics, a syntax checker for scripts and
CI, and line-numbered error messages throughout.

## Highlights

- Terminal IDE with BASIC line numbers in the gutter, syntax highlighting,
  visible text selection, clipboard support, unsaved-change protection and
  atomic saves.
- Programs run in a separate window, so a stuck program can be closed without
  taking the editor down. Where no window can be opened (macOS, SSH, no
  desktop) the program runs full-screen in the IDE's terminal instead.
- Every error names its line: `Line 12: FOR without NEXT`,
  `Line 30 (BASIC line 300): Division by zero`. On a syntax error the IDE
  moves the cursor to that line.
- `check` validates a program before it runs: syntax, undefined labels,
  unknown SUBs, jumps into blocks and string/number mix-ups.
- Graphics (`SCREEN`, `PSET`, `LINE`, `CIRCLE`, `PAINT`) open a window;
  keys pressed there reach `INKEY$`.
- Tested on Windows, macOS and Linux in CI, with tag-based release packaging.

## Requirements

- Rust 1.88 or newer.
- A terminal that supports alternate-screen applications.
- Linux graphics builds need the X11 development packages:
  `sudo apt-get install libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libxext-dev`

## Quick Start

The commands below work in any shell, including PowerShell and `cmd`.

```sh
cargo run                                        # open the IDE
cargo run -- run examples/control_flow.bas       # run a program
cargo run -- check examples/data_read_restore.bas # validate without running
cargo build --release                            # optimized binary in target/release/
```

New to programming or BASIC? Start with the
[QBasic Studio Beginner Tutorial](docs/tutorial.md).

## Command Line

```text
qbasic_interpreter                 Launch the terminal IDE
qbasic_interpreter <file.bas>      Run a BASIC program
qbasic_interpreter run <file.bas>  Run a BASIC program
qbasic_interpreter check <file>    Parse and validate a program
qbasic_interpreter edit [file]     Open the IDE, optionally with a file
```

`run` and `check` exit with status 1 on any error, so they can be used in
scripts. When standard input is piped, `INPUT` reads lines from it and echoes
them, and screen-control codes from `CLS`, `LOCATE` and `COLOR` are left out
of the output.

## IDE Shortcuts

| Shortcut | Action |
| --- | --- |
| `F5`, `Ctrl+R` | Run the current program |
| `F6` | Close the running program window |
| `F2`, `Ctrl+S` | Save |
| `F12` | Save as |
| `F3`, `Ctrl+O` | Open (asks first if there are unsaved changes) |
| `Ctrl+N` | New program (asks first if there are unsaved changes) |
| `Shift`+arrows | Select text |
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Copy, cut, paste |
| `Ctrl+U` | Undo |
| `Esc`, `Ctrl+Q` | Quit (asks first if there are unsaved changes) |

The gutter shows BASIC line numbers in steps of 10. Lines without an explicit
number run with those virtual numbers, so `GOTO 20` works without typing every
number.

While a program runs, `Ctrl+C` stops it.

### Where programs run

| Platform | Run window |
| --- | --- |
| Windows | A new console window |
| Linux with a desktop | A new terminal window (`x-terminal-emulator`, GNOME Terminal, Konsole, Xfce Terminal, Alacritty, kitty, foot or xterm) |
| macOS, SSH sessions, no desktop | Full-screen in the IDE's own terminal; the editor returns when the program ends |

Two environment variables change this:

- `QBASIC_STUDIO_RUN=inline` always runs programs in the IDE's terminal.
- `QBASIC_STUDIO_TERMINAL="wezterm start --"` uses a specific terminal command
  (the program and its arguments are appended).

## Language Support

QBasic Studio covers the parts of classic QBasic that most programs use.

- **Program structure**: line numbers and labels, `:` statement separators,
  `'` and `REM` comments, `SUB`/`FUNCTION` (with `STATIC`), `DECLARE`,
  `CALL`, `DEF FN`, `TYPE ... END TYPE` records (including `STRING * n`
  fields), `CONST`, `OPTION BASE`, `DEFINT`/`DEFLNG`/`DEFSNG`/`DEFDBL`/`DEFSTR`.
- **Variables and arrays**: type suffixes `$ % & ! #`, `DIM` with `AS` types
  and `lower TO upper` bounds, `DIM SHARED`, `SHARED`, `STATIC`, `REDIM`
  (`PRESERVE`), `ERASE`, arrays of records, implicit 0-10 arrays, `SWAP`.
  `INTEGER` and `LONG` values are rounded and range-checked as in QBasic.
- **Control flow**: `IF`/`ELSEIF`/`ELSE` (block and single-line), `SELECT CASE`
  (`IS`, `TO`, lists), `FOR`/`NEXT` (including `NEXT J, I`), `WHILE`/`WEND`,
  `DO`/`LOOP` with `WHILE`/`UNTIL`, `EXIT FOR/DO/WHILE/SUB/FUNCTION`, `GOTO`,
  `GOSUB`/`RETURN`, `ON ... GOTO/GOSUB`, `END`, `STOP`, `SYSTEM`.
  One-line blocks such as `FOR I = 1 TO 3: PRINT I: NEXT` work.
- **Error handling**: `ON ERROR GOTO`, `RESUME`, `RESUME NEXT`,
  `RESUME label`, `ERROR n`, `ERR`, `ERL`.
- **Input and output**: `PRINT` (with `;`, `,` zones, `TAB`, `SPC` and `?`
  shorthand), `PRINT USING`, `WRITE`, `INPUT`, `LINE INPUT`, `INKEY$`,
  `INPUT$`, `CLS`, `LOCATE`, `COLOR`, `BEEP`, `SLEEP`, `DATA`/`READ`/`RESTORE`.
- **Files**: `OPEN ... FOR INPUT/OUTPUT/APPEND/RANDOM`, `CLOSE`, `PRINT #`,
  `WRITE #`, `INPUT #`, `LINE INPUT #`, `GET`/`PUT` with typed records,
  `EOF`, `LOF`, `FREEFILE`, `KILL`.
- **Graphics**: `SCREEN 1, 2, 7, 8, 9, 10, 11, 12, 13` (`SCREEN 0` returns to text),
  `PSET`, `LINE` (including `-(x, y)`, `B`, `BF`), `CIRCLE`, `PAINT`, `COLOR`.
- **Operators**: `+ - * / \ ^ MOD`, comparisons, `AND OR XOR NOT`, string
  concatenation. Literals include `1.5E3`, `&HFF`, `&O17` and `&B101`.
- **Built-in functions**: `ABS ASC ATN CDBL CHR$ CINT CLNG COS CSNG CSRLIN
  CSTR$ DATE$ EOF ERL ERR EXP FIX FREEFILE HEX$ INKEY$ INPUT$ INSTR INT
  LBOUND LCASE$ LEFT$ LEN LOF LOG LTRIM$ MID$ OCT$ POS RIGHT$ RND RTRIM$ SGN
  SIN SPACE$ SPC SQR STR$ STRING$ TAB TAN TIME$ TIMER TRIM$ UBOUND UCASE$
  VAL`, plus the `MID$` statement. `RND`, `TIMER` and other no-argument
  functions can be written with or without `()`. `CHR$` uses code page 437,
  so box-drawing characters display as they did in DOS.

### Differences from Microsoft QBasic

- Numbers print without QBasic's leading and trailing spaces:
  `PRINT 1; 2` shows `12`, not ` 1  2 `.
- Text printed while a graphics screen is active goes to the terminal, not
  the graphics window.
- `TIMER`, `DATE$` and `TIME$` use UTC.
- `CIRCLE` arcs and aspect ratios, `DRAW`, `PLAY`, `SOUND`, `BINARY` files and
  multi-line `DEF FN` are not supported. `WINDOW` is accepted and ignored.
- Recursion is limited to 1000 nested calls, after which the program stops
  with "Out of stack space".

## Development

Use the same commands that CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

`cargo test` includes the golden-file tests in `tests/golden.rs`, which run
every program in `tests/programs/` through the real binary:

- `NAME.bas` is the program.
- `NAME.out` is the exact expected output.
- `NAME.in` (optional) is fed to standard input.
- `NAME.err` marks a program that must fail; its error must contain this text.

To add a regression test, write the `.bas` file, check its output by hand and
save it as `.out` (or the expected error as `.err`).

## CI/CD

The workflow in `.github/workflows/ci-cd.yml`:

- Checks formatting, runs clippy and all tests on Windows, macOS and Linux, and
  validates the examples.
- Builds with the minimum supported Rust version from `Cargo.toml`.
- On a version tag, checks that the tag matches the `Cargo.toml` version, builds
  release archives for all three platforms with SHA-256 checksums, and publishes
  a GitHub Release.

Dependency updates for Cargo and GitHub Actions are configured in
`.github/dependabot.yml`.

## Release Process

1. Update `version` in `Cargo.toml`, then run `cargo build` so `Cargo.lock`
   picks up the new version.
2. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
   `cargo test --all-targets`.
3. Commit and push.
4. Tag the release with the same version and push the tag:

```sh
git tag v0.4.0
git push origin v0.4.0
```

GitHub Actions refuses to publish if the tag and `Cargo.toml` disagree.

## Project Structure

```text
src/main.rs         CLI, terminal IDE and the console/graphics front end
src/lexer.rs        Tokenizer
src/parser.rs       AST, parser and pre-run validation
src/interpreter.rs  Runtime evaluator
src/format.rs       Number formatting and PRINT USING
src/graphics.rs     Frame buffer and drawing primitives
tests/              Golden-file tests and their BASIC programs
docs/               User guides and tutorials
examples/           Runnable BASIC sample programs
.github/            CI/CD and dependency automation
```

## License

QBasic Studio is released under the [MIT License](LICENSE).
