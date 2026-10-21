//! A stand-in for shellcheck and pyflakes in the tests of `workflint_linter::tool`.
//!
//! The first argument picks what it does:
//!
//! - `echo`: copies stdin to stdout and to stderr.
//! - `args ARG...`: prints each argument on its own line.
//! - `env NAME`: prints the value of the environment variable `NAME`.
//! - `exit CODE [TEXT]`: prints `TEXT` to stdout if given, `failed` to stderr, and exits with `CODE`.
//! - `interleave`: prints `out 1` to stdout, `err` to stderr, and `out 2` to stdout, in that order.
//! - `sleep`: sleeps for two minutes, longer than any test waits.
//! - `pidfile DIR`: writes its process ID to a file in `DIR`, then sleeps like `sleep`.
//! - `overlap DIR`: creates a file in `DIR`, waits half a second, prints how many files `DIR`
//!   holds, and deletes its file. The largest count is how many ran at once.
//! - `pool DIR N`: runs `N` copies of `pidfile DIR` at once through a `ToolPool` and waits for them.
//! - `kill-self`: sends itself SIGKILL with the `kill` command (Unix only).
//! - `record DIR ARG...`: writes the arguments after `DIR`, one per line, then a line `--`, then
//!   stdin, to a new file in `DIR`, and prints `[]`, shellcheck's output for no findings.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::ffi::OsString;
use std::io::{Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs, io, process, thread};

use workflint_linter::tool::{RunOptions, ToolCommand, ToolPool};

const LONG_SLEEP: Duration = Duration::from_secs(120);

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    match args.first().map(String::as_str) {
        Some("echo") => echo(),
        Some("args") => {
            for arg in rest {
                println!("{arg}");
            }
        }
        Some("env") => println!("{}", env::var(rest[0]).unwrap_or_default()),
        Some("exit") => {
            if let Some(text) = rest.get(1) {
                print!("{text}");
            }
            eprint!("failed");
            process::exit(rest[0].parse().unwrap());
        }
        Some("interleave") => {
            println!("out 1");
            io::stdout().flush().unwrap();
            eprintln!("err");
            println!("out 2");
        }
        Some("sleep") => thread::sleep(LONG_SLEEP),
        Some("pidfile") => {
            fs::write(Path::new(rest[0]).join(process::id().to_string()), "").unwrap();
            thread::sleep(LONG_SLEEP);
        }
        Some("overlap") => overlap(Path::new(rest[0])),
        Some("pool") => pool(Path::new(rest[0]), rest[1].parse().unwrap()),
        Some("record") => record(Path::new(rest[0]), &rest[1..]),
        Some("kill-self") => {
            process::Command::new("kill")
                .args(["-KILL", &process::id().to_string()])
                .status()
                .unwrap();
            thread::sleep(LONG_SLEEP);
        }
        other => panic!("unknown mode {other:?}"),
    }
}

fn echo() {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).unwrap();
    // Write both streams in chunks, so that a parent that reads one stream to the end before the
    // other fills the other pipe and deadlocks.
    for chunk in input.chunks(4096) {
        io::stdout().write_all(chunk).unwrap();
        io::stderr().write_all(chunk).unwrap();
    }
}

fn record(dir: &Path, args: &[&str]) {
    let mut text = String::new();
    for arg in args {
        text.push_str(arg);
        text.push('\n');
    }
    text.push_str("--\n");
    io::stdin().read_to_string(&mut text).unwrap();
    fs::write(dir.join(process::id().to_string()), text).unwrap();
    print!("[]");
}

fn overlap(dir: &Path) {
    let mine = dir.join(process::id().to_string());
    fs::write(&mine, "").unwrap();
    thread::sleep(Duration::from_millis(500));
    print!("{}", fs::read_dir(dir).unwrap().count());
    fs::remove_file(mine).unwrap();
}

fn pool(dir: &Path, n: usize) {
    let exe = env::current_exe().unwrap();
    let command = ToolCommand::resolve(exe.to_str().unwrap()).unwrap();
    let pool = ToolPool::new(NonZeroUsize::new(n).unwrap()).unwrap();
    let dir: PathBuf = dir.to_path_buf();
    thread::scope(|scope| {
        for _ in 0..n {
            scope.spawn(|| {
                let args = [OsString::from("pidfile"), dir.clone().into_os_string()];
                pool.run(&command, &args, b"", &RunOptions::default())
            });
        }
    });
}
