//! The command-line flags, parsed as Go's `flag` package parses actionlint's (ADR 0003).
//!
//! [`FLAGS`] declares every flag once: its name, its help text, and the field of [`Options`]
//! it sets. [`parse`] reads the command line with it, [`write_defaults`] prints the flag list
//! of `-help` from it, and the shell completions and the man page list the flags from it. To
//! add a flag, add its field to [`Options`] and its entry to [`FLAGS`].

use std::ffi::OsString;
use std::fmt::Write as _;
use std::num::NonZeroUsize;

use workflint_diagnostics::quote::quote;

/// The values of the flags.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // One field per boolean flag.
pub(crate) struct Options {
    pub(crate) color: bool,
    pub(crate) config_file: String,
    pub(crate) debug: bool,
    pub(crate) format: String,
    pub(crate) ignore: Vec<String>,
    pub(crate) init_config: bool,
    /// `-jobs`, or `None` for one tool process per CPU.
    pub(crate) jobs: Option<NonZeroUsize>,
    pub(crate) no_color: bool,
    pub(crate) oneline: bool,
    /// The `-output` values, `FORMAT=PATH`, in order.
    pub(crate) output: Vec<String>,
    /// `-pyflakes`, or `None` when the flag is not given and `python.linter` picks the Python
    /// linter.
    pub(crate) pyflakes: Option<String>,
    pub(crate) quiet: bool,
    /// `-shellcheck`, or `None` when the flag is not given and shellcheck runs if it is found.
    pub(crate) shellcheck: Option<String>,
    pub(crate) shellcheck_cache: bool,
    pub(crate) silent: bool,
    pub(crate) stdin_filename: String,
    pub(crate) summary: bool,
    pub(crate) verbose: bool,
    pub(crate) version: bool,
}

impl Default for Options {
    /// The values of the flags when none is given. A string flag's default is the one that
    /// [`FLAGS`] shows in the help.
    fn default() -> Self {
        let default_of = |name: &str| {
            FLAGS
                .iter()
                .find(|flag| flag.name == name)
                .and_then(|flag| flag.default)
                .unwrap_or_default()
                .to_owned()
        };
        Self {
            color: false,
            config_file: String::new(),
            debug: false,
            format: String::new(),
            ignore: Vec::new(),
            init_config: false,
            jobs: None,
            no_color: false,
            oneline: false,
            output: Vec::new(),
            pyflakes: None,
            quiet: false,
            shellcheck: None,
            shellcheck_cache: false,
            silent: false,
            stdin_filename: default_of("stdin-filename"),
            summary: false,
            verbose: false,
            version: false,
        }
    }
}

/// What a flag sets.
#[derive(Clone, Copy)]
enum Setter {
    /// A boolean flag. It takes a value only after `=`, as in `-oneline=false`.
    Bool(fn(&mut Options, bool)),
    /// A flag with a value, after `=` or as the next argument.
    String(fn(&mut Options, String)),
    /// A flag with a positive whole number, after `=` or as the next argument.
    Count(fn(&mut Options, NonZeroUsize)),
}

/// What the shell completions offer for the value of a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Values {
    /// Nothing, or `true` and `false` for a boolean flag after `=`.
    Any,
    /// File names.
    File,
    /// Command names and file names.
    Command,
    /// The names of the output formats.
    Format,
}

/// One flag.
pub(crate) struct Flag {
    /// The name, without the leading `-`.
    pub(crate) name: &'static str,
    /// The name of the value in the help, such as `string`. Empty for a boolean flag.
    pub(crate) value_name: &'static str,
    /// The default that the help shows, if any.
    pub(crate) default: Option<&'static str>,
    /// The help text.
    pub(crate) usage: &'static str,
    /// What the shell completions offer for the value.
    pub(crate) values: Values,
    set: Setter,
}

impl Flag {
    const fn bool(name: &'static str, usage: &'static str, set: fn(&mut Options, bool)) -> Self {
        Self {
            name,
            value_name: "",
            default: None,
            usage,
            values: Values::Any,
            set: Setter::Bool(set),
        }
    }

    const fn string(
        name: &'static str,
        default: Option<&'static str>,
        usage: &'static str,
        set: fn(&mut Options, String),
    ) -> Self {
        Self {
            name,
            value_name: "string",
            default,
            usage,
            values: Values::Any,
            set: Setter::String(set),
        }
    }

    /// A flag with a positive whole number. Go's help names its value `int`.
    const fn count(
        name: &'static str,
        usage: &'static str,
        set: fn(&mut Options, NonZeroUsize),
    ) -> Self {
        Self {
            name,
            value_name: "int",
            default: None,
            usage,
            values: Values::Any,
            set: Setter::Count(set),
        }
    }

    /// A flag that may be given more than once. Go's help names its value `value`.
    const fn list(name: &'static str, usage: &'static str, set: fn(&mut Options, String)) -> Self {
        Self {
            name,
            value_name: "value",
            default: None,
            usage,
            values: Values::Any,
            set: Setter::String(set),
        }
    }

    /// The flag, with the completions offering `values` for its value.
    const fn completing(self, values: Values) -> Self {
        Self { values, ..self }
    }

    /// Whether the flag is boolean, and so takes a value only after `=`.
    pub(crate) const fn is_bool(&self) -> bool {
        matches!(self.set, Setter::Bool(_))
    }

    /// The help text without the sentences after the first.
    pub(crate) fn summary(&self) -> &'static str {
        self.usage
            .split_once(". ")
            .map_or(self.usage, |(first, _)| first)
    }
}

/// Every flag: actionlint's, and then workflint's own.
pub(crate) static FLAGS: [Flag; 21] = [
    Flag::list(
        "ignore",
        "Regular expression in Go syntax. Errors whose message it matches are not reported. \
         This flag is repeatable",
        |options, pattern| options.ignore.push(pattern),
    ),
    Flag::string(
        "shellcheck",
        Some("shellcheck"),
        "Command name or file path of \"shellcheck\" external command. If empty, shellcheck \
         integration will be disabled",
        |options, value| options.shellcheck = Some(value),
    )
    .completing(Values::Command),
    Flag::string(
        "pyflakes",
        Some("pyflakes"),
        "Command name or file path of \"pyflakes\" external command. If empty, pyflakes \
         integration will be disabled. If given, pyflakes checks Python scripts whatever \
         python.linter says",
        |options, value| options.pyflakes = Some(value),
    )
    .completing(Values::Command),
    Flag::bool(
        "oneline",
        "Use one line per one error. Useful for reading error messages from programs",
        |options, on| options.oneline = on,
    ),
    Flag::string(
        "format",
        None,
        "Custom template to format error messages in Go template syntax, or the name of a \
         format: github, json, jsonl, sarif, or summary. The command-line documentation lists \
         the template's fields and functions",
        |options, value| options.format = value,
    )
    .completing(Values::Format),
    Flag::string(
        "config-file",
        None,
        "File path to config file. It replaces the config file of each project",
        |options, value| options.config_file = value,
    )
    .completing(Values::File),
    Flag::bool(
        "init-config",
        "Generate default config file at .github/workflint.yaml in current project",
        |options, on| options.init_config = on,
    ),
    Flag::bool("no-color", "Disable colorful output", |options, on| {
        options.no_color = on;
    }),
    Flag::bool(
        "color",
        "Always enable colorful output, even when NO_COLOR is set or stdout is not a terminal",
        |options, on| options.color = on,
    ),
    Flag::bool("verbose", "Enable verbose output", |options, on| {
        options.verbose = on;
    }),
    Flag::bool(
        "debug",
        "Enable debug output (for development). It includes the verbose output",
        |options, on| options.debug = on,
    ),
    Flag::bool(
        "version",
        "Show version and how this binary was installed",
        |options, on| options.version = on,
    ),
    Flag::string(
        "stdin-filename",
        Some("<stdin>"),
        "File name when reading input from stdin",
        |options, value| options.stdin_filename = value,
    )
    .completing(Values::File),
    Flag::bool(
        "quiet",
        "Print only errors and fatal messages: no summary and no logs",
        |options, on| options.quiet = on,
    ),
    Flag::bool("q", "Same as -quiet", |options, on| options.quiet = on),
    Flag::string("config", None, "Same as -config-file", |options, value| {
        options.config_file = value;
    })
    .completing(Values::File),
    Flag::bool(
        "silent",
        "Print only fatal messages. The exit code tells whether errors were found",
        |options, on| options.silent = on,
    ),
    Flag::bool(
        "summary",
        "Print the number of errors by severity to stderr",
        |options, on| options.summary = on,
    ),
    Flag::count(
        "jobs",
        "Maximum number of external tool processes, such as shellcheck, that run at once. The \
         default is the number of CPUs",
        |options, jobs| options.jobs = Some(jobs),
    ),
    Flag::bool(
        "shellcheck-cache",
        "Keep the findings of shellcheck in a cache on disk, so that a later run does not check \
         the same script again. The config key shellcheck.cache does the same for one project",
        |options, on| options.shellcheck_cache = on,
    ),
    Flag::list(
        "output",
        "Also write the errors to a file, as FORMAT=PATH, such as sarif=results.sarif. FORMAT is \
         github, json, jsonl, sarif, or summary. The summary format appends to the file. This \
         flag is repeatable",
        |options, value| options.output.push(value),
    ),
];

/// The command line, parsed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parsed<'a> {
    /// `-h` or `-help`: print the usage and exit with 0.
    Help,
    /// `completion`: print a completion script, for the shell that these arguments name.
    Completion(&'a [OsString]),
    /// Run with these flags on these arguments. The flags are boxed, so that the result stays
    /// small.
    Run(Box<Options>, &'a [OsString]),
}

/// Parses the arguments after the program name.
///
/// A leading `completion` takes the rest of the arguments. A leading `check` is the default
/// command and is skipped. Then the flags are parsed as Go's
/// `flag` package does. A flag starts with `-` or `--`. A boolean flag takes its value only
/// after `=`, and another flag takes it after `=` or from the next argument. The flags end at
/// `--` or at the first argument that is not a flag, such as `-` or a file. `-h` and `-help`
/// stop the parse.
///
/// # Errors
///
/// Fails with Go's message for an invalid command line.
pub(crate) fn parse(args: &[OsString]) -> Result<Parsed<'_>, String> {
    let mut options = Options::default();
    let mut rest = match args.split_first() {
        Some((first, rest)) if first == "completion" => return Ok(Parsed::Completion(rest)),
        Some((first, rest)) if first == "check" => rest,
        _ => args,
    };
    while let Some((arg, tail)) = rest.split_first() {
        let arg = arg.to_string_lossy();
        let Some(name) = arg.strip_prefix('-').filter(|name| !name.is_empty()) else {
            break;
        };
        rest = tail;
        if name == "-" {
            break;
        }
        let name = name.strip_prefix('-').unwrap_or(name);
        if name.is_empty() || name.starts_with(['-', '=']) {
            return Err(format!("bad flag syntax: {arg}"));
        }
        let (name, value) = match name.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (name, None),
        };
        let Some(flag) = FLAGS.iter().find(|flag| flag.name == name) else {
            if name == "h" || name == "help" {
                return Ok(Parsed::Help);
            }
            return Err(format!("flag provided but not defined: -{name}"));
        };
        match flag.set {
            Setter::Bool(set) => {
                let on = match value {
                    Some(value) => parse_bool(value).ok_or_else(|| {
                        format!(
                            "invalid boolean value {} for -{name}: parse error",
                            quote(value)
                        )
                    })?,
                    None => true,
                };
                set(&mut options, on);
            }
            Setter::String(set) => set(&mut options, take_value(name, value, &mut rest)?),
            Setter::Count(set) => {
                let value = take_value(name, value, &mut rest)?;
                set(&mut options, parse_count(name, &value)?);
            }
        }
    }
    Ok(Parsed::Run(Box::new(options), rest))
}

/// The value of the flag `name`: `value`, the text after `=`, or else the next argument, which
/// it takes off `rest`.
fn take_value(name: &str, value: Option<&str>, rest: &mut &[OsString]) -> Result<String, String> {
    if let Some(value) = value {
        return Ok(value.to_owned());
    }
    let Some((value, tail)) = rest.split_first() else {
        return Err(format!("flag needs an argument: -{name}"));
    };
    *rest = tail;
    Ok(value.to_string_lossy().into_owned())
}

/// Parses the value of the count flag `name`. The error has the form of Go's for an invalid
/// flag value.
fn parse_count(name: &str, value: &str) -> Result<NonZeroUsize, String> {
    let reason = match value.parse::<usize>() {
        Ok(count) => match NonZeroUsize::new(count) {
            Some(count) => return Ok(count),
            None => "must be at least 1",
        },
        Err(_) => "parse error",
    };
    Err(format!(
        "invalid value {} for flag -{name}: {reason}",
        quote(value)
    ))
}

/// Parses a boolean flag value as Go's `strconv.ParseBool` does.
fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
        _ => None,
    }
}

/// Writes the flags sorted by name, with their help and defaults, as Go's
/// `FlagSet.PrintDefaults` does.
pub(crate) fn write_defaults(out: &mut String) {
    let mut flags: Vec<&Flag> = FLAGS.iter().collect();
    flags.sort_by_key(|flag| flag.name);
    for flag in flags {
        let start = out.len();
        let _ = write!(out, "  -{}", flag.name);
        if !flag.value_name.is_empty() {
            let _ = write!(out, " {}", flag.value_name);
        }
        // A one-letter boolean flag fits before the tab stop.
        out.push_str(if out.len() - start <= 4 {
            "\t"
        } else {
            "\n    \t"
        });
        out.push_str(&flag.usage.replace('\n', "\n    \t"));
        if let Some(default) = flag.default.filter(|default| !default.is_empty()) {
            let _ = write!(out, " (default {})", quote(default));
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_run(args: &[&str]) -> Result<(Options, Vec<String>), String> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        match parse(&args)? {
            Parsed::Help => Err("help".to_owned()),
            Parsed::Completion(_) => Err("completion".to_owned()),
            Parsed::Run(options, files) => Ok((
                *options,
                files
                    .iter()
                    .map(|file| file.to_string_lossy().into_owned())
                    .collect(),
            )),
        }
    }

    fn files(args: &[&str]) -> Vec<String> {
        parse_run(args).map(|(_, files)| files).unwrap_or_default()
    }

    fn options(args: &[&str]) -> Options {
        parse_run(args)
            .map(|(options, _)| options)
            .unwrap_or_default()
    }

    #[test]
    fn a_lone_dash_is_a_file_and_ends_the_flags() {
        assert_eq!(files(&["-oneline", "-", "-no-color"]), ["-", "-no-color"]);
        assert!(options(&["-oneline", "-", "-no-color"]).oneline);
    }

    #[test]
    fn three_dashes_or_a_name_starting_with_equals_is_bad_flag_syntax() {
        for arg in ["---oneline", "-=x", "--=x"] {
            assert_eq!(parse_run(&[arg]), Err(format!("bad flag syntax: {arg}")));
        }
    }

    #[test]
    fn a_later_boolean_flag_overrides_an_earlier_one() {
        assert!(!options(&["-oneline", "-oneline=0", "a.yaml"]).oneline);
    }

    #[test]
    fn boolean_values_are_the_ones_go_accepts() {
        for value in ["1", "t", "T", "TRUE", "true", "True"] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }
        for value in ["0", "f", "F", "FALSE", "false", "False"] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }
        for value in ["", "yes", "tRUE", " true"] {
            assert_eq!(parse_bool(value), None, "{value}");
        }
    }

    #[test]
    fn a_string_flag_takes_the_next_argument_even_if_it_starts_with_a_dash() {
        let (options, files) =
            parse_run(&["-config-file", "-oneline", "a.yaml"]).unwrap_or_default();
        assert_eq!(options.config_file, "-oneline");
        assert!(!options.oneline);
        assert_eq!(files, ["a.yaml"]);
    }

    #[test]
    fn ignore_collects_every_pattern_in_order() {
        assert_eq!(
            options(&["-ignore", "a", "--ignore=b", "-ignore="]).ignore,
            ["a", "b", ""]
        );
    }

    #[test]
    fn output_collects_every_value_in_order() {
        assert_eq!(
            options(&["-output", "sarif=a", "--output=json=b"]).output,
            ["sarif=a", "json=b"]
        );
    }

    #[test]
    fn every_flag_sets_its_own_field() {
        let set = options(&[
            "-color",
            "-config-file=c",
            "-debug",
            "-format={{.}}",
            "-ignore=i",
            "-init-config",
            "-jobs=3",
            "-no-color",
            "-oneline",
            "-output=json=o",
            "-pyflakes=p",
            "-q",
            "-shellcheck=s",
            "-shellcheck-cache",
            "-silent",
            "-stdin-filename=n",
            "-summary",
            "-verbose",
            "-version",
        ]);
        assert_eq!(
            set,
            Options {
                color: true,
                config_file: "c".to_owned(),
                debug: true,
                format: "{{.}}".to_owned(),
                ignore: vec!["i".to_owned()],
                init_config: true,
                jobs: NonZeroUsize::new(3),
                no_color: true,
                oneline: true,
                output: vec!["json=o".to_owned()],
                pyflakes: Some("p".to_owned()),
                quiet: true,
                shellcheck: Some("s".to_owned()),
                shellcheck_cache: true,
                silent: true,
                stdin_filename: "n".to_owned(),
                summary: true,
                verbose: true,
                version: true,
            }
        );
        assert!(options(&["-quiet"]).quiet);
    }

    #[test]
    fn string_flags_default_to_the_reference_values() {
        let defaults = Options::default();
        assert_eq!(defaults.stdin_filename, "<stdin>");
        for name in ["shellcheck", "pyflakes"] {
            let flag = FLAGS.iter().find(|flag| flag.name == name);
            assert_eq!(flag.and_then(|flag| flag.default), Some(name));
        }
    }

    #[test]
    fn shellcheck_is_none_unless_given_and_may_be_empty() {
        assert_eq!(options(&[]).shellcheck, None);
        assert_eq!(options(&["-shellcheck="]).shellcheck, Some(String::new()));
        assert_eq!(
            options(&["-shellcheck", "shellcheck"]).shellcheck,
            Some("shellcheck".to_owned())
        );
    }

    #[test]
    fn pyflakes_is_none_unless_given_and_may_be_empty() {
        assert_eq!(options(&[]).pyflakes, None);
        assert_eq!(options(&["-pyflakes="]).pyflakes, Some(String::new()));
        assert_eq!(
            options(&["-pyflakes", "pyflakes"]).pyflakes,
            Some("pyflakes".to_owned())
        );
    }

    #[test]
    fn jobs_takes_a_positive_whole_number() {
        assert_eq!(options(&["-jobs", "2"]).jobs, NonZeroUsize::new(2));
        assert_eq!(options(&["--jobs=16"]).jobs, NonZeroUsize::new(16));
        for (value, reason) in [
            ("0", "must be at least 1"),
            ("x", "parse error"),
            ("-1", "parse error"),
            ("", "parse error"),
        ] {
            assert_eq!(
                parse_run(&[&format!("-jobs={value}")]),
                Err(format!(
                    "invalid value \"{value}\" for flag -jobs: {reason}"
                ))
            );
        }
        assert_eq!(
            parse_run(&["-jobs"]),
            Err("flag needs an argument: -jobs".to_owned())
        );
    }

    #[test]
    fn h_and_help_stop_the_parse_whatever_follows() {
        for args in [
            &["-h"][..],
            &["--help"],
            &["-oneline", "-h", "-bogus"],
            &["-help=x"],
            &["check", "-h"],
        ] {
            assert_eq!(parse_run(args), Err("help".to_owned()), "{args:?}");
        }
    }

    #[test]
    fn a_leading_completion_takes_the_other_arguments() {
        assert_eq!(
            parse_run(&["completion", "-x"]),
            Err("completion".to_owned())
        );
        assert_eq!(files(&["-oneline", "completion"]), ["completion"]);
        assert_eq!(files(&["check", "completion"]), ["completion"]);
    }

    #[test]
    fn a_leading_check_is_the_default_command() {
        assert_eq!(files(&["check", "-oneline", "a.yaml"]), ["a.yaml"]);
        assert_eq!(files(&["-oneline", "check", "a.yaml"]), ["check", "a.yaml"]);
    }

    /// The lines of actionlint's `-help` for the flags whose help text workflint keeps.
    #[test]
    fn flag_defaults_print_as_go_prints_them() {
        let mut out = String::new();
        write_defaults(&mut out);
        for expected in [
            "  -no-color\n    \tDisable colorful output\n",
            "  -oneline\n    \tUse one line per one error. Useful for reading error messages \
             from programs\n",
            "  -verbose\n    \tEnable verbose output\n",
            "  -version\n    \tShow version and how this binary was installed\n",
            "  -ignore value\n    \t",
            "  -config-file string\n    \t",
            "  -stdin-filename string\n    \tFile name when reading input from stdin \
             (default \"<stdin>\")\n",
            "  -q\tSame as -quiet\n",
            "  -jobs int\n    \t",
        ] {
            assert!(out.contains(expected), "{expected:?} not in\n{out}");
        }
    }

    #[test]
    fn flag_defaults_are_sorted_by_name() {
        let mut out = String::new();
        write_defaults(&mut out);
        let names: Vec<&str> = out
            .lines()
            .filter_map(|line| line.strip_prefix("  -"))
            .map(|line| line.split(['\t', ' ']).next().unwrap_or(line))
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert_eq!(names.len(), FLAGS.len());
    }
}
