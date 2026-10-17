//! `uses` of steps and jobs: a non-empty string with placeholders,
//! classified.

use workflint_diagnostics::SourceFile;

use super::{MapEntry, Parser};
use crate::{Field, JobUses, StepUses, Template};

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
        classify: fn(Template<'a>, &SourceFile) -> T,
    ) -> Field<'a, T> {
        self.parse_template(entry.key.string, entry.value, false)
            .map(|template| classify(template, self.file))
    }
}
