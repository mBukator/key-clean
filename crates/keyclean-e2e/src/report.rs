//! The results table, printed and written as Markdown.

use std::path::PathBuf;
use std::time::Duration;

use crate::scenarios::Outcome;

struct Row {
    id: &'static str,
    steps: &'static str,
    name: &'static str,
    status: &'static str,
    detail: String,
}

pub struct Report {
    rows: Vec<Row>,
}

impl Report {
    pub fn new() -> Self {
        Report { rows: Vec::new() }
    }

    pub fn add(
        &mut self,
        id: &'static str,
        steps: &'static str,
        name: &'static str,
        outcome: Outcome,
    ) {
        let (status, detail) = match outcome {
            Outcome::Pass(d) => ("PASS", d),
            Outcome::Fail(d) => ("FAIL", d),
            Outcome::Skip(d) => ("SKIP", d),
        };
        self.rows.push(Row {
            id,
            steps,
            name,
            status,
            detail,
        });
    }

    pub fn failed(&self) -> usize {
        self.rows.iter().filter(|r| r.status == "FAIL").count()
    }

    fn count(&self, status: &str) -> usize {
        self.rows.iter().filter(|r| r.status == status).count()
    }

    pub fn markdown(&self, elapsed: Duration) -> String {
        let mut out = String::from("# KeyClean e2e results (M1)\n\n");
        out.push_str("| ID | M1 step | Check | Result | Detail |\n");
        out.push_str("| -- | ------- | ----- | ------ | ------ |\n");
        for r in &self.rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                r.id,
                r.steps,
                r.name,
                r.status,
                r.detail.replace('|', "/")
            ));
        }
        out.push_str(&format!(
            "\n{} passed, {} failed, {} skipped in {:.0} s.\n",
            self.count("PASS"),
            self.count("FAIL"),
            self.count("SKIP"),
            elapsed.as_secs_f64()
        ));
        out
    }

    pub fn print(&self, elapsed: Duration) {
        println!();
        print!("{}", self.markdown(elapsed));
    }

    /// Writes `e2e-report.md` next to the harness binary (`target/<profile>/`).
    pub fn write_next_to_exe(&self, elapsed: Duration) -> Option<PathBuf> {
        let path = std::env::current_exe()
            .ok()?
            .parent()?
            .join("e2e-report.md");
        std::fs::write(&path, self.markdown(elapsed)).ok()?;
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_counts_and_escapes() {
        let mut r = Report::new();
        r.add("S1", "1", "List", Outcome::Pass("2 keyboards".into()));
        r.add("S2", "2", "Chord", Outcome::Fail("a | b".into()));
        r.add("S3", "3", "App", Outcome::Skip("not built".into()));
        let md = r.markdown(Duration::from_secs(5));
        assert!(md.contains("| S2 | 2 | Chord | FAIL | a / b |"));
        assert!(md.contains("1 passed, 1 failed, 1 skipped"));
        assert_eq!(r.failed(), 1);
    }
}
