//! `uses` of steps and jobs: a non-empty string, classified.

use workflint_diagnostics::SourceFile;

use super::{MapEntry, Parser};
use crate::{Field, JobUses, StepUses, Str};

impl<'a> Parser<'a, '_> {
    /// Parses `uses` of a step, a string that must not be empty, and
    /// classifies it.
    pub(super) fn parse_step_uses(&mut self, entry: &MapEntry<'a>) -> Field<'a, StepUses<'a>> {
        self.parse_uses(entry, StepUses::new)
    }

    /// Parses `uses` of a job, a string that must not be empty, and
    /// classifies it.
    pub(super) fn parse_job_uses(&mut self, entry: &MapEntry<'a>) -> Field<'a, JobUses<'a>> {
        self.parse_uses(entry, JobUses::new)
    }

    fn parse_uses<T>(
        &mut self,
        entry: &MapEntry<'a>,
        classify: fn(Str<'a>, &SourceFile) -> T,
    ) -> Field<'a, T> {
        match self.parse_string(entry.key.string, entry.value, false) {
            Field::Valid { key, span, value } => Field::Valid {
                key,
                span,
                value: classify(value, self.file),
            },
            Field::Invalid { key, span } => Field::Invalid { key, span },
            Field::Absent => Field::Absent,
        }
    }
}
