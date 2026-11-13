//! Golden-file tests: every `tests/programs/NAME.bas` is run with the real
//! binary and checked against its expectations.
//!
//! * `NAME.in`  - optional text fed to standard input.
//! * `NAME.out` - the exact expected standard output.
//! * `NAME.err` - for programs that must fail: text the error message must
//!   contain. The program must exit with a failure status.
//!
//! Programs run in a fresh temporary directory, so they may create files.
//! To add a case, write the `.bas` file, check its output by hand, and save
//! it as `.out` (or the expected error as `.err`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn programs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs")
}

fn run_program(path: &Path, stdin: &str) -> (bool, String, String) {
    let workdir = tempfile::tempdir().expect("temp dir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_qbasic_interpreter"))
        .arg("run")
        .arg(path)
        .current_dir(workdir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start interpreter");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child
        .wait_with_output()
        .expect("interpreter did not finish");
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n");
    (out.status.success(), norm(&out.stdout), norm(&out.stderr))
}

#[test]
fn golden_programs() {
    let mut cases: Vec<PathBuf> = fs::read_dir(programs_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bas"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "no golden programs found");

    let mut failures = Vec::new();
    for bas in &cases {
        let name = bas.file_name().unwrap().to_string_lossy().to_string();
        let stdin = fs::read_to_string(bas.with_extension("in")).unwrap_or_default();
        let (ok, stdout, stderr) = run_program(bas, &stdin);
        if let Ok(expected_err) = fs::read_to_string(bas.with_extension("err")) {
            let expected_err = expected_err.trim();
            if ok || !stderr.contains(expected_err) {
                failures.push(format!(
                    "{name}: expected failure containing {expected_err:?}\n  success: {ok}\n  stderr: {stderr}"
                ));
            }
            continue;
        }
        let expected = fs::read_to_string(bas.with_extension("out"))
            .unwrap_or_else(|_| panic!("{name} has neither a .out nor a .err file"))
            .replace("\r\n", "\n");
        if !ok || stdout != expected || !stderr.is_empty() {
            failures.push(format!(
                "{name}:\n--- expected ---\n{expected}--- actual (success: {ok}) ---\n{stdout}--- stderr ---\n{stderr}"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// The bundled examples must run cleanly (graphics excluded: it needs a
/// window and waits for SLEEP).
#[test]
fn examples_run() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    for (file, stdin, expected_tail) in [
        ("hello.bas", "Ada\n", "Nice to meet you, Ada.\n"),
        ("control_flow.bas", "", "Double of 21 is 42\n"),
        (
            "data_read_restore.bas",
            "",
            "First record after RESTORE: ADA\n",
        ),
    ] {
        let (ok, stdout, stderr) = run_program(&examples.join(file), stdin);
        assert!(ok, "{file} failed: {stderr}");
        assert!(stdout.ends_with(expected_tail), "{file} printed:\n{stdout}");
    }
}

#[test]
fn check_command_reports_errors_with_line_numbers() {
    let output = Command::new(env!("CARGO_BIN_EXE_qbasic_interpreter"))
        .arg("check")
        .arg(programs_dir().join("err_missing_next.bas"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Line 2: FOR without NEXT"), "{stderr}");
}
