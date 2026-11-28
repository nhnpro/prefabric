//! Cobertura XML coverage reports (Coverlet / `dotnet test --collect:"XPlat Code Coverage"`,
//! Python `coverage xml`, etc.):
//!
//! ```xml
//! <coverage>
//!   <sources>
//!     <source>/path/to/repo</source>
//!   </sources>
//!   <packages>
//!     <package name="MyApp">
//!       <classes>
//!         <class name="MyApp.Calc" filename="src/Calc.cs">
//!           <lines>
//!             <line number="10" hits="3" branch="true" condition-coverage="50% (1/2)"/>
//! ```

use crate::Accumulator;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 5;

/// Cobertura XML reports below `dir` (`dotnet test --results-directory` writes `<guid>/coverage.cobertura.xml`).
pub(crate) fn find_reports(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) {
            out.push(path);
        }
    }
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).map(|a| a.value.into_owned())
}

/// Parses `"50% (1/2)"` into `Some((1, 2))`.
fn parse_condition_coverage(raw: &str) -> Option<(u32, u32)> {
    let open = raw.find('(')?;
    let close = raw[open + 1..].find(')')?;
    let inner = &raw[open + 1..open + 1 + close];
    let (covered, total) = inner.split_once('/')?;
    Some((covered.trim().parse().ok()?, total.trim().parse().ok()?))
}

/// Resolves a Cobertura `filename` against `<sources>` and `base`.
fn resolve_file(filename: &str, sources: &[PathBuf], base: &Path) -> PathBuf {
    let normalized = if cfg!(windows) { filename.to_owned() } else { filename.replace('\\', "/") };
    let rel = Path::new(&normalized);
    if rel.is_absolute() {
        return rel.to_path_buf();
    }
    for src in sources {
        let candidate = src.join(rel);
        if candidate.is_file() {
            return candidate;
        }
    }
    let from_base = base.join(rel);
    if from_base.is_file() {
        return from_base;
    }
    sources.first().map(|s| s.join(rel)).unwrap_or(from_base)
}

/// Parses one Cobertura XML report into `acc`, resolving relative paths against `<sources>` and `base`.
pub(crate) fn parse_into(xml: &str, base: &Path, acc: &mut Accumulator) -> Result<(), quick_xml::Error> {
    let mut reader = Reader::from_str(xml);
    let mut sources: Vec<PathBuf> = Vec::new();
    let mut in_source = false;
    let mut source_buf = String::new();
    let mut file: Option<PathBuf> = None;

    loop {
        match reader.read_event()? {
            Event::Start(e) => match e.name().as_ref() {
                "source" => {
                    in_source = true;
                    source_buf.clear();
                }
                "class" => {
                    if let Some(filename) = attr(&e, "filename").filter(|s| !s.is_empty()) {
                        let resolved = resolve_file(&filename, &sources, base);
                        acc.file(&resolved);
                        file = Some(resolved);
                    } else {
                        file = None;
                    }
                }
                "line" => record_line(&e, file.as_deref(), acc),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "class" => {
                    if let Some(filename) = attr(&e, "filename").filter(|s| !s.is_empty()) {
                        let resolved = resolve_file(&filename, &sources, base);
                        acc.file(&resolved);
                    }
                }
                "line" => record_line(&e, file.as_deref(), acc),
                _ => {}
            },
            Event::Text(e) if in_source => {
                source_buf.push_str(e.as_ref());
            }
            Event::End(e) => match e.name().as_ref() {
                "source" => {
                    in_source = false;
                    let trimmed = source_buf.trim();
                    if !trimmed.is_empty() {
                        sources.push(PathBuf::from(trimmed));
                    }
                    source_buf.clear();
                }
                "class" => file = None,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn record_line(e: &BytesStart, file: Option<&Path>, acc: &mut Accumulator) {
    let Some(f) = file else { return };
    let Some(nr) = attr(e, "number").and_then(|v| v.parse::<u32>().ok()).filter(|&n| n > 0) else {
        return;
    };
    let hits = attr(e, "hits").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    acc.line(f, nr - 1, hits);
    let is_branch = attr(e, "branch").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if is_branch
        && let Some(cond) = attr(e, "condition-coverage")
        && let Some((covered, total)) = parse_condition_coverage(&cond)
        && total > 0
    {
        acc.has_branches = true;
        for i in 0..total {
            acc.branch(f, nr - 1, i.to_string(), i < covered);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COBERTURA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<coverage line-rate="0.75" branch-rate="0.5" version="1.9">
  <sources>
    <source>/repo</source>
  </sources>
  <packages>
    <package name="MyApp">
      <classes>
        <class name="MyApp.Calc" filename="src/Calc.cs">
          <lines>
            <line number="5" hits="3" branch="false"/>
            <line number="6" hits="1" branch="true" condition-coverage="50% (1/2)"/>
            <line number="9" hits="0" branch="false"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;

    #[test]
    fn parses_lines_branches_and_sources() {
        let mut acc = Accumulator::default();
        parse_into(COBERTURA, Path::new("/fallback"), &mut acc).unwrap();
        let suite = acc.finish("dotnet test (Coverlet)");
        assert!(suite.has_branches);
        assert_eq!(suite.files.len(), 1);
        let calc = &suite.files[0];
        assert_eq!(calc.path.as_ref() as &Path, Path::new("/repo/src/Calc.cs"));
        assert_eq!((calc.lines_total, calc.lines_covered), (3, 2));
        assert_eq!((calc.branches_total, calc.branches_covered), (2, 1));
        assert_eq!(calc.lines[0].line, 4);
        assert_eq!(calc.lines[0].hits, 3);
        assert!(calc.lines[1].partial());
        assert_eq!(calc.lines[2].hits, 0);
    }

    #[test]
    fn finds_nested_coverlet_reports() {
        let dir = tempfile::tempdir().unwrap();
        let guid = dir.path().join("abc-123");
        std::fs::create_dir_all(&guid).unwrap();
        std::fs::write(guid.join("coverage.cobertura.xml"), COBERTURA).unwrap();
        std::fs::write(guid.join("notes.txt"), "ignore").unwrap();
        let found = find_reports(dir.path());
        assert_eq!(found, vec![guid.join("coverage.cobertura.xml")]);
    }
}
