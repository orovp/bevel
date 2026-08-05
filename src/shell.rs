//! Running a command line that a human wrote.
//!
//! Pack steps (`cargo clippy --all-targets -- -D warnings`), fix steps and
//! `key_command` are all written as shell lines rather than argv arrays,
//! deliberately: a pack is edited by hand, and `&&` and `|` are the reason.
//! That means an interpreter, and the interpreter is the one thing that varies
//! by platform.
//!
//! `cmd /C` on Windows rather than PowerShell, because it is what npm runs
//! scripts with — so a line that already works in `package.json` works here —
//! and because it is the only shell guaranteed present. The price is that a
//! step must stay inside the portable subset: `&&` and `|` are shared, but a
//! leading `FOO=1` or single quotes are not. Every built-in pack step is a bare
//! `cargo` or `npm` invocation and crosses unharmed.

use std::process::Command;

/// A shell prepared to run `line`, for the caller to add a working directory to
/// and then `status()` or `output()`.
#[cfg(windows)]
pub fn command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    // `raw_arg`, not `arg`: Rust quotes arguments by the C runtime's rules, and
    // cmd.exe does not read those — it would see the backslash escapes around
    // any quote in the line as literal characters. Passing the line verbatim is
    // what makes it behave as typed.
    cmd.arg("/C").raw_arg(line);
    cmd
}

#[cfg(not(windows))]
pub fn command(line: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(line);
    cmd
}

#[cfg(test)]
mod tests {
    /// Whatever the shell is, a pack step has to come back with its exit code:
    /// `verify` turns that number into a pass or a fail, and a step that cannot
    /// be spawned at all is the failure mode this module exists to prevent.
    #[test]
    fn a_command_line_runs_and_reports_its_exit_code() {
        let ok = super::command("exit 0").status().unwrap();
        assert!(ok.success());
        let bad = super::command("exit 3").status().unwrap();
        assert_eq!(bad.code(), Some(3));
    }
}
