use crate::*;
use std::fmt::{self, Write};

pub(crate) struct LimitedWriter {
    text: String,
    remaining: usize,
    truncated: bool,
}
impl LimitedWriter {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            remaining: limit,
            truncated: false,
        }
    }
    pub(crate) fn finish(mut self) -> String {
        if self.truncated {
            self.text.push('…');
        }
        self.text
    }
}
impl fmt::Write for LimitedWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() <= self.remaining {
            self.text.push_str(s);
            self.remaining -= s.len();
            Ok(())
        } else {
            let mut end = self.remaining.min(s.len());
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&s[..end]);
            self.remaining = 0;
            self.truncated = true;
            Err(fmt::Error)
        }
    }
}
pub fn bounded_debug(value: &impl fmt::Debug, limit: usize) -> String {
    let mut out = LimitedWriter::new(limit);
    let _ = write!(out, "{value:?}");
    out.finish()
}
pub(crate) fn bounded_display(value: &impl fmt::Display, limit: usize) -> String {
    let mut out = LimitedWriter::new(limit);
    let _ = write!(out, "{value}");
    out.finish()
}
/// Diagnostic budgets cap retained/rendered evidence independently of source size.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticLimits {
    pub failures: usize,
    pub characters_per_failure: usize,
}
impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            failures: 64,
            characters_per_failure: 2048,
        }
    }
}
/// One bounded failure with a semantic section path.
#[derive(Clone, Debug)]
pub struct CheckFailure {
    pub section: String,
    pub name: String,
    pub class: CheckClass,
    pub diagnostic: String,
}
/// Collected proof status. Successful checks retain no measured report or trace.
#[derive(Clone, Debug)]
pub struct CertificationReport {
    pub name: String,
    pub checks: usize,
    pub failed: usize,
    pub failures: Vec<CheckFailure>,
}
impl CertificationReport {
    pub fn is_pass(&self) -> bool {
        self.failed == 0
    }
    #[track_caller]
    pub fn assert_certify(&self) {
        if !self.is_pass() {
            panic!("{self}");
        }
    }
}
impl fmt::Display for CertificationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_pass() {
            return Ok(());
        }
        writeln!(
            f,
            "CERTIFICATION FAILED: {} ({} / {} checks)",
            self.name, self.failed, self.checks
        )?;
        for failure in &self.failures {
            let class = match failure.class {
                CheckClass::Hard => "HARD",
                CheckClass::Statistical => "STATISTICAL",
                CheckClass::Error => "ERROR",
                CheckClass::Pass => "PASS",
            };
            writeln!(
                f,
                "{}
  [{}] {}
    {}",
                failure.section, class, failure.name, failure.diagnostic
            )?;
        }
        if self.failed > self.failures.len() {
            writeln!(
                f,
                "{} additional failures omitted",
                self.failed - self.failures.len()
            )?;
        }
        Ok(())
    }
}
/// A named semantic proof; sections/checks collect independently until the final assertion.
pub struct Certification {
    report: CertificationReport,
    path: String,
    limits: DiagnosticLimits,
}
pub fn certify(name: impl Into<String>) -> Certification {
    Certification {
        report: CertificationReport {
            name: bounded_display(&name.into(), 512),
            checks: 0,
            failed: 0,
            failures: Vec::new(),
        },
        path: String::new(),
        limits: DiagnosticLimits::default(),
    }
}
impl Certification {
    pub fn limits(&mut self, limits: DiagnosticLimits) -> &mut Self {
        self.limits = limits;
        self
    }
    pub fn source<'a, T>(
        &self,
        name: impl Into<std::sync::Arc<str>>,
        value: &'a T,
    ) -> PreparedSource<'a, T> {
        source(name, value)
    }
    pub fn collection<'a, T>(
        &self,
        name: impl Into<std::sync::Arc<str>>,
        values: &'a [T],
    ) -> PreparedSource<'a, T> {
        source_slice(name, values)
    }
    pub fn check<T: fmt::Debug, V: fmt::Debug>(
        &mut self,
        name: impl Into<String>,
        result: CompareResult<T, V>,
    ) -> &mut Self {
        self.report.checks += 1;
        if result.is_pass() {
            return self;
        }
        self.report.failed += 1;
        if self.report.failures.len() < self.limits.failures {
            let mut out = LimitedWriter::new(self.limits.characters_per_failure);
            let _ = write!(
                out,
                "sources: {}; expected: {}; ",
                result.source, result.expected
            );
            if !result.details.is_empty() {
                let _ = write!(out, "{}; ", result.details);
            }
            let _ = write!(
                out,
                "analysis error: {:?}; violations: {:?}; observed: {:?}",
                result.analysis_error, result.violations, result.observed
            );
            self.report.failures.push(CheckFailure {
                section: bounded_display(&self.path, 512),
                name: bounded_display(&name.into(), 256),
                class: result.class,
                diagnostic: out.finish(),
            });
        }
        self
    }
    pub fn comparison<T: fmt::Debug, V: fmt::Debug>(
        &mut self,
        name: impl Into<String>,
        result: CompareResult<T, V>,
    ) -> &mut Self {
        self.check(name, result)
    }
    pub fn section(
        &mut self,
        name: impl Into<String>,
        checks: impl FnOnce(&mut Certification),
    ) -> &mut Self {
        let mut child = certify(self.report.name.clone());
        child.limits = DiagnosticLimits {
            failures: self
                .limits
                .failures
                .saturating_sub(self.report.failures.len()),
            ..self.limits
        };
        child.path = bounded_display(
            &format!("{}{}/", self.path, bounded_display(&name.into(), 256)),
            512,
        );
        checks(&mut child);
        self.report.checks += child.report.checks;
        self.report.failed += child.report.failed;
        self.report.failures.extend(child.report.failures);
        self
    }
    pub fn certify(&self) -> CertificationReport {
        self.report.clone()
    }
    #[track_caller]
    pub fn assert_certify(&self) {
        self.report.assert_certify();
    }
}
