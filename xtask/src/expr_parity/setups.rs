//! The setups in `tests/expr-parity/setups.toml`: the environments of the
//! checker that the expressions run in.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use workflint_data::{UNTRUSTED_INPUTS, WORKFLOW_KEYS};
use workflint_expr::TypeEnv;

use super::types::{self, TypeSpec};

/// The `key` of a setup that stands for every workflow key.
pub(crate) const EVERY_KEY: &str = "*";

/// A workflow key that no table lists, which allows no context and no special
/// function. A setup with [`EVERY_KEY`] runs with it too.
pub(crate) const UNKNOWN_KEY: &str = "jobs.<job_id>.unknown";

/// The expression sets that a setup can name in `expressions`.
pub(crate) const EXPRESSION_SETS: [&str; 4] = ["reference", "generated", "cases", "availability"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    setup: Vec<Entry>,
}

/// One `[[setup]]` table.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    key: String,
    expressions: Vec<String>,
    #[serde(default)]
    untrusted: bool,
    config_vars: Option<Vec<String>>,
    matrix: Option<String>,
    steps: Option<String>,
    needs: Option<String>,
    secrets: Option<String>,
    jobs: Option<String>,
    #[serde(default)]
    inputs: Vec<String>,
    dispatch_inputs: Option<String>,
}

/// One environment of the checker. The Go program reads it as JSON.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Setup {
    /// The name in reports and in the allow-list. A setup with
    /// [`EVERY_KEY`] becomes one setup per key, named `name (key)`.
    pub(crate) name: String,
    /// The workflow key whose contexts and special functions are allowed.
    pub(crate) key: String,
    /// Whether the untrusted-input check is on, as in a script.
    pub(crate) untrusted: bool,
    /// The configured variable names, or `None` to allow any `vars.NAME`.
    pub(crate) config_vars: Option<Vec<String>>,
    pub(crate) matrix: Option<TypeSpec>,
    pub(crate) steps: Option<TypeSpec>,
    pub(crate) needs: Option<TypeSpec>,
    /// The declared secrets. Both checkers add the built-in ones.
    pub(crate) secrets: Option<TypeSpec>,
    pub(crate) jobs: Option<TypeSpec>,
    /// The inputs of each trigger, added in order.
    pub(crate) inputs: Vec<TypeSpec>,
    /// The inputs of `workflow_dispatch`, added after `inputs`.
    pub(crate) dispatch_inputs: Option<TypeSpec>,
    /// The names of the expression sets to run, from [`EXPRESSION_SETS`].
    #[serde(skip)]
    pub(crate) expressions: Vec<String>,
}

impl Setup {
    /// The `workflint-expr` environment of the setup.
    pub(crate) fn type_env(&self) -> TypeEnv {
        let mut env = TypeEnv::new();
        if let Some(names) = &self.config_vars {
            env.set_config_vars(names.iter().cloned());
        }
        if let Some(ty) = &self.matrix {
            env.set_matrix(ty.to_object());
        }
        if let Some(ty) = &self.steps {
            env.set_steps(ty.to_object());
        }
        if let Some(ty) = &self.needs {
            env.set_needs(ty.to_object());
        }
        if let Some(ty) = &self.secrets {
            env.set_secrets(ty.to_object());
        }
        if let Some(ty) = &self.jobs {
            env.set_jobs(ty.to_object());
        }
        for ty in &self.inputs {
            env.add_inputs(ty.to_object());
        }
        if let Some(ty) = &self.dispatch_inputs {
            env.add_dispatch_inputs(ty.to_object());
        }
        env.set_workflow_key(&self.key);
        if self.untrusted {
            env.set_untrusted_inputs(UNTRUSTED_INPUTS);
        }
        env
    }
}

/// Parses the setups file, and expands each setup with [`EVERY_KEY`] into one
/// setup for each key in `workflint-data` and one for [`UNKNOWN_KEY`].
pub(crate) fn parse(text: &str) -> Result<Vec<Setup>> {
    let file: File = toml::from_str(text).context("cannot parse the setups")?;
    let mut setups = Vec::new();
    for entry in file.setup {
        let setup = convert(&entry).with_context(|| format!("in setup {:?}", entry.name))?;
        if entry.key == EVERY_KEY {
            let keys = WORKFLOW_KEYS
                .iter()
                .map(|key| key.name)
                .chain([UNKNOWN_KEY]);
            for key in keys {
                setups.push(Setup {
                    name: format!("{} ({key})", entry.name),
                    key: key.to_owned(),
                    ..setup.clone()
                });
            }
        } else {
            setups.push(setup);
        }
    }
    let mut names = BTreeSet::new();
    for setup in &setups {
        if !names.insert(setup.name.as_str()) {
            bail!("two setups are named {:?}", setup.name);
        }
    }
    Ok(setups)
}

fn convert(entry: &Entry) -> Result<Setup> {
    if entry.expressions.is_empty() {
        bail!("`expressions` is empty");
    }
    for set in &entry.expressions {
        if !EXPRESSION_SETS.contains(&set.as_str()) {
            bail!("unknown expression set {set:?}, expected one of {EXPRESSION_SETS:?}");
        }
    }
    let object = |field: &str, text: &Option<String>| -> Result<Option<TypeSpec>> {
        text.as_deref()
            .map(|text| object_type(field, text))
            .transpose()
    };
    Ok(Setup {
        name: entry.name.clone(),
        key: entry.key.clone(),
        untrusted: entry.untrusted,
        config_vars: entry.config_vars.clone(),
        matrix: object("matrix", &entry.matrix)?,
        steps: object("steps", &entry.steps)?,
        needs: object("needs", &entry.needs)?,
        secrets: object("secrets", &entry.secrets)?,
        jobs: object("jobs", &entry.jobs)?,
        inputs: entry
            .inputs
            .iter()
            .map(|text| object_type("inputs", text))
            .collect::<Result<_>>()?,
        dispatch_inputs: object("dispatch_inputs", &entry.dispatch_inputs)?,
        expressions: entry.expressions.clone(),
    })
}

/// Parses the type `text` of the field `field`, which must be an object.
fn object_type(field: &str, text: &str) -> Result<TypeSpec> {
    let ty = types::parse(text).with_context(|| format!("in `{field}`"))?;
    if !ty.is_object() {
        bail!("`{field}` must be an object type, but it is {text:?}");
    }
    Ok(ty)
}

#[cfg(test)]
mod tests {
    use workflint_expr::parse_expression;

    use super::*;

    fn check(setup: &Setup, expression: &str) -> Vec<String> {
        let (expr, _) = parse_expression(&format!("{expression}}}}}")).unwrap();
        let (_, errors) = setup.type_env().check(&expr);
        errors.into_iter().map(|error| error.message).collect()
    }

    #[test]
    fn a_setup_builds_the_environment_it_describes() {
        let setups = parse(
            r#"
[[setup]]
name = "run"
key = "jobs.<job_id>.steps.run"
expressions = ["cases"]
untrusted = true
config_vars = ["known"]
matrix = "{os: string}"
inputs = ["{a: string}"]
dispatch_inputs = "{b: bool}"
"#,
        )
        .unwrap();
        let [setup] = setups.as_slice() else {
            panic!("{setups:?}");
        };
        assert_eq!(
            check(setup, "matrix.os && inputs.a && inputs.b && vars.KNOWN"),
            Vec::<String>::new()
        );
        assert_eq!(
            check(setup, "matrix.arch"),
            [r#"property "arch" is not defined in object type {os: string}"#]
        );
        assert!(check(setup, "vars.other")[0].starts_with("undefined configuration variable"));
        assert_eq!(check(setup, "github.event.inputs.b"), Vec::<String>::new());
        assert!(check(setup, "github.head_ref")[0].contains("potentially untrusted"));
        assert!(check(setup, "success()")[0].starts_with("calling function"));
    }

    #[test]
    fn every_key_expands_into_one_setup_per_workflow_key_and_an_unknown_key() {
        let setups = parse(
            r#"
[[setup]]
name = "availability"
key = "*"
expressions = ["availability"]
"#,
        )
        .unwrap();
        assert_eq!(setups.len(), WORKFLOW_KEYS.len() + 1);
        assert_eq!(
            setups[0].name,
            format!("availability ({})", WORKFLOW_KEYS[0].name)
        );
        let unknown = setups.last().unwrap();
        assert_eq!(unknown.key, UNKNOWN_KEY);
        assert!(check(unknown, "github")[0].contains("no context is available here"));
    }

    #[test]
    fn malformed_setups_are_errors() {
        for (text, expected) in [
            (
                "[[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = []\n",
                "is empty",
            ),
            (
                "[[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = [\"x\"]\n",
                "unknown expression set",
            ),
            (
                "[[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = [\"cases\"]\nmatrix = \"string\"\n",
                "must be an object type",
            ),
            (
                "[[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = [\"cases\"]\nother = 1\n",
                "cannot parse",
            ),
            (
                "[[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = [\"cases\"]\n\
                 [[setup]]\nname = \"a\"\nkey = \"env\"\nexpressions = [\"cases\"]\n",
                "two setups",
            ),
            (
                "[[setup]]\nname = \"a\"\nexpressions = [\"cases\"]\n",
                "cannot parse",
            ),
        ] {
            let error = format!("{:#}", parse(text).unwrap_err());
            assert!(error.contains(expected), "{error}");
        }
    }
}
