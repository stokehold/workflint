//! A stand-in for shellcheck and pyflakes in the tests of `workflint_linter::tool`.
//!
//! The first argument picks what it does:
//!
//! - `echo`: copies stdin to stdout and to stderr.
//! - `args ARG...`: prints each argument on its own line.
//! - `env NAME`: prints the value of the environment variable `NAME`.
//! - `exit CODE [TEXT]`: prints `TEXT` to stdout if given, `failed` to stderr, and exits with `CODE`.
//! - `say CODE STDOUT STDERR`: prints `STDOUT` to stdout and `STDERR` to stderr, and exits with
//!   `CODE`.
//! - `interleave`: prints `out 1` to stdout, `err` to stderr, and `out 2` to stdout, in that order.
//! - `sleep`: sleeps for two minutes, longer than any test waits.
//! - `pidfile DIR`: writes its process ID to a file in `DIR`, then sleeps like `sleep`.
//! - `overlap DIR`: creates a file in `DIR`, waits half a second, prints how many files `DIR`
//!   holds, and deletes its file. The largest count is how many ran at once.
//! - `pool DIR N`: runs `N` copies of `pidfile DIR` at once through a `ToolPool` and waits for them.
//! - `kill-self`: sends itself SIGKILL with the `kill` command (Unix only).
//! - `record DIR ARG...`: writes the arguments after `DIR`, one per line, then a line `--`, then
//!   stdin, to a new file in `DIR`, and prints `[]`, shellcheck's output for no findings. When
//!   the arguments name script files in place of `-`, as a batch does, it writes one such file
//!   for each script, with the script files left out of the arguments, `-` after them, and the
//!   script in place of stdin. Each file is named `PID-N`, so the files of one process share
//!   the prefix.
//! - `version TEXT MODE...`: prints shellcheck's `--version` output for version `TEXT` when the
//!   arguments contain `--version`, and otherwise does what `MODE...` does.
//! - `per-file`: prints one finding for each script, at line 2, column 1, whose message is the
//!   script's second line, the first line of the `run:` value. A script on stdin is file `-`.
//! - `no-batch MODE...`: fails with exit code 3 and no output when the arguments name script
//!   files, and otherwise does what `MODE...` does.

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
    run(&args.iter().map(String::as_str).collect::<Vec<_>>());
}

fn run(args: &[&str]) {
    let rest: Vec<&str> = args.iter().skip(1).copied().collect();
    match args.first().copied() {
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
        Some("say") => {
            print!("{}", rest[1]);
            io::stdout().flush().unwrap();
            eprint!("{}", rest[2]);
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
        Some("version") => {
            if rest.contains(&"--version") {
                println!(
                    "ShellCheck - shell script analysis tool\nversion: {}\nlicense: GNU General \
                     Public License, version 3\nwebsite: https://www.shellcheck.net",
                    rest[0]
                );
            } else {
                run(&rest[1..]);
            }
        }
        Some("per-file") => per_file(&rest),
        Some("no-batch") => {
            if script_files(&rest).1.is_empty() {
                run(&rest);
            } else {
                process::exit(3);
            }
        }
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

/// The arguments without the script files, and the script files: the arguments that name
/// files, except the value of `--rcfile`.
fn script_files<'a>(args: &[&'a str]) -> (Vec<&'a str>, Vec<&'a str>) {
    let (mut flags, mut files) = (Vec::new(), Vec::new());
    let mut previous = "";
    for &arg in args {
        if previous != "--rcfile" && Path::new(arg).is_file() {
            files.push(arg);
        } else {
            flags.push(arg);
        }
        previous = arg;
    }
    (flags, files)
}

fn record(dir: &Path, args: &[&str]) {
    let (mut flags, files) = script_files(args);
    let scripts: Vec<String> = if files.is_empty() {
        let mut stdin = String::new();
        io::stdin().read_to_string(&mut stdin).unwrap();
        vec![stdin]
    } else {
        flags.push("-");
        files
            .iter()
            .map(|file| fs::read_to_string(file).unwrap())
            .collect()
    };
    for (index, script) in scripts.iter().enumerate() {
        let mut text = String::new();
        for arg in &flags {
            text.push_str(arg);
            text.push('\n');
        }
        text.push_str("--\n");
        text.push_str(script);
        fs::write(dir.join(format!("{}-{index}", process::id())), text).unwrap();
    }
    print!("[]");
}

fn per_file(args: &[&str]) {
    let (_, files) = script_files(args);
    let scripts: Vec<(String, String)> = if files.is_empty() {
        let mut stdin = String::new();
        io::stdin().read_to_string(&mut stdin).unwrap();
        vec![("-".to_owned(), stdin)]
    } else {
        files
            .iter()
            .map(|file| ((*file).to_owned(), fs::read_to_string(file).unwrap()))
            .collect()
    };
    let findings: Vec<String> = scripts
        .iter()
        .map(|(file, script)| {
            let message = script.lines().nth(1).unwrap_or_default();
            format!(
                r#"{{"file":{},"line":2,"endLine":2,"column":1,"endColumn":2,"level":"info","code":1000,"message":{}}}"#,
                serde_json::to_string(file).unwrap(),
                serde_json::to_string(message).unwrap()
            )
        })
        .collect();
    print!("[{}]", findings.join(","));
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
