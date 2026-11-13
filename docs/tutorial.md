# QBasic Studio Beginner Tutorial

This tutorial is for people who are new to programming. It teaches QBasic Studio
as both an editor and a programming language. You will write small programs,
run them, read errors, and gradually build programs that ask questions, make
decisions, repeat work, store data, use files, and draw graphics.

You do not need to know Rust to use QBasic Studio. Rust is used to build the
IDE and interpreter, but the programs in this tutorial are BASIC programs.

## What You Will Learn

By the end, you should be comfortable with:

- Starting the IDE.
- Typing, saving, opening, and running BASIC programs.
- Reading the status bar when something goes wrong.
- Printing text and numbers.
- Asking the user for input.
- Using variables, strings, and calculations.
- Making decisions with `IF`.
- Repeating work with `FOR`, `WHILE`, and `DO`.
- Using arrays, `DATA`, functions, and subroutines.
- Writing and reading text files.
- Drawing simple graphics.
- Choosing a small practice project and finishing it.

## 1. Start The IDE

From the project folder, run:

```sh
cargo run
```

If you already built the release executable with `cargo build --release`, you
can also run it directly: `target/release/qbasic_interpreter` (on Windows,
`target\release\qbasic_interpreter.exe`).

The IDE has three main areas:

- Header: shows the app name, current file, saved/modified state, and run state.
- Editor: where you type BASIC code.
- Status bar: shows messages, errors, and common shortcuts.

The left gutter shows BASIC line numbers such as `10`, `20`, and `30`. You do
not usually need to type those numbers yourself. QBasic Studio adds usable
virtual line numbers when you run from the IDE.

## 2. Essential Shortcuts

| Shortcut | What it does |
| --- | --- |
| `F5`, `Ctrl+R` | Run the current program in a separate window |
| `F6` | Stop the running program window |
| `F2`, `Ctrl+S` | Save |
| `F12` | Save as |
| `F3`, `Ctrl+O` | Open |
| `Ctrl+N` | Start a new program |
| `Shift`+arrows | Select text |
| `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | Copy, cut, paste |
| `Ctrl+U` | Undo |
| `Esc`, `Ctrl+Q` | Quit |

When you press `F5`, your BASIC program opens in its own program window. This
keeps the IDE usable if your program waits for input or gets stuck in a loop.
On macOS, or when there is no desktop (for example over SSH), the program runs
full-screen in the same terminal instead and the editor comes back when it
ends.

If a program gets stuck, press `Ctrl+C` in its window to stop it. The IDE asks
before quitting, opening another file or starting a new one if you have unsaved
changes.

## 3. Your First Program

Press `Ctrl+N` to start a new program. Type this:

```basic
PRINT "Hello, world!"
PRINT "I am learning BASIC."
```

Press `F5` to run it.

`PRINT` tells the computer to show something. Text inside double quotes is a
string. A string is just text data.

Try changing the words, then run again:

```basic
PRINT "QBasic Studio is ready."
PRINT "This is my first edit."
```

If the status bar says `modified`, the editor has changes that are not saved
yet. Press `F2` to save.

## 4. Save And Open Programs

Press `F12` to save the current program with a new filename, for example:

```text
hello.bas
```

Later, press `F3` to open a file. Type the file path and press Enter.

You can also run or check a file from the command line:

```powershell
cargo run -- run examples\hello.bas
cargo run -- check examples\hello.bas
```

`check` is useful when you want to know whether the code parses without running
the program.

## 5. Comments

Comments are notes for humans. The computer ignores them.

```basic
' This program prints two lines.
PRINT "First line"
PRINT "Second line"
```

Use comments to explain why your program does something. You do not need a
comment for every line.

## 6. Variables

A variable is a named box that stores a value.

```basic
NAME$ = "Ada"
AGE = 36

PRINT NAME$
PRINT AGE
```

The `$` at the end of `NAME$` means the variable stores text. Variables without
`$` usually store numbers.

Try this:

```basic
NAME$ = "Grace"
LANGUAGE$ = "BASIC"
PRINT NAME$; " is learning "; LANGUAGE$; "."
```

The semicolon joins printed items without moving to a new line.

## 7. Input

Programs become more interesting when they can ask questions.

```basic
INPUT "What is your name? ", NAME$
PRINT "Nice to meet you, "; NAME$; "."
```

For numbers, use a variable without `$`:

```basic
INPUT "How old are you? ", AGE
NEXTAGE = AGE + 1
PRINT "On your next birthday you will be "; NEXTAGE; "."
```

If a numeric input asks for a number and you type words, the program will report
a runtime error. That is normal while learning.

## 8. Calculations

BASIC can do arithmetic:

```basic
WIDTH = 8
HEIGHT = 5
AREA = WIDTH * HEIGHT

PRINT "Width: "; WIDTH
PRINT "Height: "; HEIGHT
PRINT "Area: "; AREA
```

Common operators:

| Operator | Meaning | Example |
| --- | --- | --- |
| `+` | Add | `A + B` |
| `-` | Subtract | `A - B` |
| `*` | Multiply | `A * B` |
| `/` | Divide | `A / B` |
| `\` | Integer division | `17 \ 3` gives `5` |
| `MOD` | Remainder | `17 MOD 3` gives `2` |
| `^` | Power | `2 ^ 3` gives `8` |

Small experiment:

```basic
PRINT 17 / 3
PRINT 17 \ 3
PRINT 17 MOD 3
```

## 9. Decisions With IF

An `IF` statement lets the program choose.

```basic
INPUT "Enter a score: ", SCORE

IF SCORE >= 70 THEN
    PRINT "Passing score."
ELSE
    PRINT "Keep practicing."
END IF
```

Comparison operators:

| Operator | Meaning |
| --- | --- |
| `=` | Equal |
| `<>` | Not equal |
| `<` | Less than |
| `<=` | Less than or equal |
| `>` | Greater than |
| `>=` | Greater than or equal |

You can use `ELSEIF` for more than two choices:

```basic
INPUT "Enter a score: ", SCORE

IF SCORE >= 90 THEN
    PRINT "A"
ELSEIF SCORE >= 80 THEN
    PRINT "B"
ELSEIF SCORE >= 70 THEN
    PRINT "C"
ELSE
    PRINT "Needs practice"
END IF
```

## 10. Loops

A loop repeats work.

### FOR/NEXT

Use `FOR` when you know how many times you want to repeat.

```basic
FOR I = 1 TO 5
    PRINT "Count: "; I
NEXT I
```

You can use `STEP` to count by a different amount:

```basic
FOR I = 10 TO 2 STEP -2
    PRINT I
NEXT I
```

### WHILE/WEND

Use `WHILE` when you want to repeat while something is true.

```basic
COUNT = 1

WHILE COUNT <= 5
    PRINT COUNT
    COUNT = COUNT + 1
WEND
```

### DO/LOOP

Use `DO` when the loop shape reads better this way:

```basic
COUNT = 1

DO
    PRINT COUNT
    COUNT = COUNT + 1
LOOP UNTIL COUNT > 5
```

If a loop never stops, press `F6` in the IDE or close the program window.

## 11. Mini Project: Guess The Number

This program combines input, variables, decisions, and loops.

```basic
RANDOMIZE TIMER()

SECRET = INT(RND() * 10) + 1
GUESS = 0
TRIES = 0

PRINT "I picked a number from 1 to 10."

WHILE GUESS <> SECRET
    INPUT "Your guess: ", GUESS
    TRIES = TRIES + 1

    IF GUESS < SECRET THEN
        PRINT "Too low."
    ELSEIF GUESS > SECRET THEN
        PRINT "Too high."
    ELSE
        PRINT "Correct in "; TRIES; " tries."
    END IF
WEND
```

Things to notice:

- `RANDOMIZE TIMER()` seeds the random number generator.
- `RND()` returns a random number between 0 and 1.
- `INT(...)` removes the decimal part.
- The loop continues until `GUESS` equals `SECRET`.

Challenge: Change the game so the secret number is from 1 to 100.

## 12. Strings

Strings are text values. String variables usually end with `$`.

```basic
FIRST$ = "ada"
LAST$ = "lovelace"

PRINT UCASE$(FIRST$)
PRINT UCASE$(LAST$)
PRINT "Initials: "; LEFT$(FIRST$, 1); LEFT$(LAST$, 1)
```

Useful string functions:

| Function | What it does |
| --- | --- |
| `LEN(TEXT$)` | Counts characters |
| `LEFT$(TEXT$, N)` | Gets the left part |
| `RIGHT$(TEXT$, N)` | Gets the right part |
| `MID$(TEXT$, START, N)` | Gets part of a string |
| `UCASE$(TEXT$)` | Converts to uppercase |
| `LCASE$(TEXT$)` | Converts to lowercase |
| `TRIM$(TEXT$)` | Removes spaces from both ends |
| `INSTR(TEXT$, FIND$)` | Finds text and returns a 1-based position |

Example:

```basic
WORD$ = "BASIC"

PRINT LEN(WORD$)
PRINT LEFT$(WORD$, 2)
PRINT MID$(WORD$, 2, 3)
PRINT INSTR(WORD$, "S")
```

## 13. DATA, READ, And RESTORE

`DATA` stores values inside your program. `READ` takes the next value.

```basic
DATA "ADA", 1815
DATA "GRACE", 1906
DATA "KATHERINE", 1918

FOR I = 1 TO 3
    READ NAME$, YEAR
    PRINT NAME$; " : "; YEAR
NEXT I
```

`RESTORE` moves back to the beginning of the data:

```basic
DATA "RED", "GREEN", "BLUE"

READ FIRST$
READ SECOND$
PRINT FIRST$; ", "; SECOND$

RESTORE
READ AGAIN$
PRINT "After RESTORE: "; AGAIN$
```

## 14. Arrays

An array stores many values under one name.

```basic
DIM SCORE(4)

SCORE(0) = 88
SCORE(1) = 92
SCORE(2) = 75
SCORE(3) = 100
SCORE(4) = 84

TOTAL = 0

FOR I = 0 TO 4
    TOTAL = TOTAL + SCORE(I)
NEXT I

AVERAGE = TOTAL / 5
PRINT "Average score: "; AVERAGE
```

In QBasic Studio, `DIM SCORE(4)` creates usable positions `0` through `4`.

You can also declare a type:

```basic
TYPE PLAYER
    NAME AS STRING
    SCORE AS INTEGER
END TYPE

DIM P AS PLAYER
P.NAME = "Ada"
P.SCORE = 42

PRINT P.NAME; " scored "; P.SCORE
```

## 15. Functions And Subroutines

A function returns a value.

```basic
FUNCTION DOUBLEIT(N AS DOUBLE) AS DOUBLE
    DOUBLEIT = N * 2
END FUNCTION

PRINT DOUBLEIT(21)
```

A subroutine performs a task.

```basic
SUB SHOWTITLE()
    PRINT "================"
    PRINT "  BASIC QUIZ"
    PRINT "================"
END SUB

CALL SHOWTITLE()
```

You can combine them:

```basic
SUB SHOWTITLE()
    PRINT "================"
    PRINT "  BASIC QUIZ"
    PRINT "================"
END SUB

FUNCTION POINTS(CORRECT AS DOUBLE) AS DOUBLE
    POINTS = CORRECT * 10
END FUNCTION

CALL SHOWTITLE()
INPUT "How many answers were correct? ", COUNT
PRINT "Score: "; POINTS(COUNT)
```

## 16. Labels, GOTO, And GOSUB

Most new programs are easier to read with `IF`, `FOR`, `WHILE`, functions, and
subroutines. Still, classic BASIC also supports labels and jumps.

```basic
START:
INPUT "Type a number from 1 to 5: ", N

IF N < 1 OR N > 5 THEN
    PRINT "Try again."
    GOTO START
END IF

PRINT "Thanks."
```

You can jump to a subroutine and return:

```basic
GOSUB GREETING
PRINT "Back in the main program."
END

GREETING:
PRINT "Hello from a subroutine."
RETURN
```

Use jumps sparingly. They are powerful, but too many jumps can make a program
hard to follow.

## 17. Files

Programs can write files:

```basic
OPEN "notes.txt" FOR OUTPUT AS #1
PRINT #1, "This file was created by BASIC."
PRINT #1, "Each PRINT writes a line."
CLOSE #1

PRINT "Wrote notes.txt"
```

Programs can read files:

```basic
OPEN "notes.txt" FOR INPUT AS #1
INPUT #1, NOTE$
CLOSE #1

PRINT "First line: "; NOTE$
```

Modes you will use most often:

| Mode | Meaning |
| --- | --- |
| `INPUT` | Read an existing file |
| `OUTPUT` | Create or replace a file |
| `APPEND` | Add to the end of a file |

The file is created relative to the folder where the program is running.

## 18. Graphics

Graphics commands open a separate graphics window.

```basic
SCREEN 12
CLS
COLOR 14, 1

LINE (40, 40)-(600, 420), 11, B
LINE (80, 80)-(560, 380), 2, BF
CIRCLE (320, 240), 90, 15
PAINT (320, 240), 4, 15
PSET (320, 240), 0

SLEEP 3
```

Graphics basics:

- `SCREEN 12` starts a graphics screen.
- `CLS` clears it.
- `COLOR foreground, background` changes the drawing colors (without a
  `SCREEN`, it changes the text colors instead).
- `PSET (x, y), color` draws one pixel.
- `LINE (x1, y1)-(x2, y2), color` draws a line.
- `LINE ..., B` draws a box.
- `LINE ..., BF` draws a filled box.
- `CIRCLE (x, y), radius, color` draws a circle.
- `PAINT (x, y), fill, border` fills an area.
- `SLEEP 3` waits three seconds (or until a key is pressed).

Drawing commands need a `SCREEN` first; without one you get
"Illegal function call". When the program ends, the graphics window stays open
until you press a key in it. Keys pressed in the graphics window reach
`INKEY$`, so you can write simple games:

```basic
SCREEN 13
X = 160
DO
    K$ = INKEY$
    IF K$ = CHR$(0) + "K" THEN X = X - 4   ' left arrow
    IF K$ = CHR$(0) + "M" THEN X = X + 4   ' right arrow
    PSET (X, 100), 14
LOOP UNTIL K$ = CHR$(27)                   ' Esc quits
```

Try changing the numbers. Coordinates start near the top-left corner.

## 19. Reading Errors

The status bar is your first clue.

Common messages:

| Message kind | What it usually means |
| --- | --- |
| Syntax error | The code is not valid BASIC, so it did not start |
| Runtime error | The program started, but something went wrong while running |
| Program exited | The separate run window ended with an error code |

Every message says which line is wrong, for example
`Syntax error: Line 4: FOR without NEXT`. When you press `F5` and there is a
syntax error, the cursor jumps to that line. Runtime errors also show the
gutter number, for example `Line 3 (BASIC line 30): Division by zero`.

Some mistakes are caught before the program runs at all: misspelled
statements (`PRNT`), a `GOTO` to a label that does not exist, and adding a
number to a string.

Good debugging habits:

- Read the first error before changing anything.
- Check quotes. Strings need both opening and closing `"`.
- Check block endings: `IF` needs `END IF`, `FOR` needs `NEXT`, `WHILE` needs `WEND`.
- Check variable types. Use `$` for strings.
- Add temporary `PRINT` lines to show what your program is doing.
- Run smaller pieces while learning.

Example broken program:

```basic
PRINT "Hello
```

The string never closes. Fix it like this:

```basic
PRINT "Hello"
```

## 20. A Good Learning Routine

When learning programming, use this loop:

1. Type a tiny program.
2. Run it.
3. Change one thing.
4. Run it again.
5. If it breaks, read the status bar and undo the last change.
6. Save when it works.

Small experiments teach faster than large unfinished programs.

## 21. Final Practice Project: Tiny Quiz

This project uses several ideas from the tutorial.

```basic
SUB TITLE()
    PRINT "================"
    PRINT "    TINY QUIZ"
    PRINT "================"
END SUB

CALL TITLE()

SCORE = 0

INPUT "What is 2 + 2? ", ANSWER
IF ANSWER = 4 THEN
    PRINT "Correct."
    SCORE = SCORE + 1
ELSE
    PRINT "Not quite."
END IF

INPUT "What is 5 * 3? ", ANSWER
IF ANSWER = 15 THEN
    PRINT "Correct."
    SCORE = SCORE + 1
ELSE
    PRINT "Not quite."
END IF

INPUT "Finish this word: BA", ENDING$
IF UCASE$(ENDING$) = "SIC" THEN
    PRINT "Correct."
    SCORE = SCORE + 1
ELSE
    PRINT "Not quite."
END IF

PRINT "Final score: "; SCORE; " out of 3"

IF SCORE = 3 THEN
    PRINT "Perfect!"
ELSEIF SCORE = 2 THEN
    PRINT "Good work."
ELSE
    PRINT "Try again and improve your score."
END IF
```

Ways to extend it:

- Add more questions.
- Store the player's name.
- Write the final score to a file.
- Use `DATA` and `READ` to store question data.
- Draw a graphics celebration for a perfect score.

## Quick Reference

### Program Structure

```basic
' Comments explain the program.
PRINT "Start"

IF 1 = 1 THEN
    PRINT "Indented code is easier to read."
END IF
```

### Variables

```basic
NAME$ = "Ada"
COUNT = 3
PRICE = 4.99
```

### Input And Output

```basic
INPUT "Name? ", NAME$
PRINT "Hello, "; NAME$
```

### Conditions

```basic
IF X > 10 THEN
    PRINT "Large"
ELSE
    PRINT "Small"
END IF
```

### Loops

```basic
FOR I = 1 TO 10
    PRINT I
NEXT I
```

```basic
WHILE X < 10
    X = X + 1
WEND
```

### Functions

```basic
FUNCTION SQUARE(N AS DOUBLE) AS DOUBLE
    SQUARE = N * N
END FUNCTION
```

### Useful Built-ins

| Built-in | Purpose |
| --- | --- |
| `INT(N)` | Whole-number floor |
| `RND` | Random number from 0 to 1 |
| `TIMER` | Seconds since midnight (UTC) |
| `INKEY$` | The key being pressed, or `""` |
| `LEN(S$)` | String length |
| `LEFT$(S$, N)` | Left part of string |
| `RIGHT$(S$, N)` | Right part of string |
| `MID$(S$, START, N)` | Middle part of string |
| `UCASE$(S$)` | Uppercase |
| `LCASE$(S$)` | Lowercase |
| `VAL(S$)` | Convert string to number |
| `STR$(N)` | Convert number to string |

Built-ins that take no arguments, such as `RND`, `TIMER` and `INKEY$`, work
with or without parentheses: `RND` and `RND()` are the same.

## What To Learn Next

Pick one project and finish it:

- Number guessing game with difficulty levels.
- Flash-card quiz.
- Simple calculator.
- Contact list saved to a file.
- Drawing program using graphics commands.
- Text adventure with rooms and choices.

Keep each version small. Save working copies often. Programming is mostly
learning how to make tiny pieces work and then connect them.
