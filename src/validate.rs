//! Config validation with file coordinates (T203).
//!
//! [`crate::config::PrxConfig::check`] knows *what* is wrong and names the
//! field it is wrong in (`route[2].service`). This module knows *where* that
//! field is in the file the operator is looking at, so the editor in the Web UI
//! can put a mark on the right line instead of printing one string at the top
//! of the page.
//!
//! The mapping comes from `toml_edit`, which keeps a span for every key it
//! parses. A path that has no span of its own — a field left at its default,
//! say — falls back to its parent table, so a mark always lands somewhere that
//! is at least related to the problem rather than on line 1.

use std::ops::Range;

use serde::Serialize;
use toml_edit::{ImDocument, Item, Table};

use crate::config::{ConfigIssue, IssueSeverity, PrxConfig};

/// One problem, addressed both by field path and by file position.
///
/// Positions are 1-based, and columns count characters rather than bytes so a
/// file with Thai comments in it still marks the right column.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: IssueSeverity,
    pub code: String,
    pub path: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// The answer to `POST /web/config/validate`.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    /// True when nothing would stop this config from being applied. Warnings
    /// do not make it false.
    pub valid: bool,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl ValidationReport {
    fn from_issues(issues: Vec<ConfigIssue>, index: &SpanIndex) -> Self {
        let (errors, warnings): (Vec<_>, Vec<_>) = issues
            .into_iter()
            .map(|issue| index.locate(issue))
            .partition(|diagnostic| diagnostic.severity == IssueSeverity::Error);

        Self {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }
}

/// Validates TOML text, returning the report and — when the config is usable —
/// the parsed config, so a caller that wants both does not parse twice.
pub fn validate_text(text: &str) -> (ValidationReport, Option<PrxConfig>) {
    let positions = Positions::new(text);

    let config = match toml::from_str::<PrxConfig>(text) {
        Ok(config) => config,
        Err(err) => {
            // A file that does not parse has exactly one problem worth
            // reporting: everything after the syntax error is guesswork.
            let span = err.span().unwrap_or(0..0);
            let (line, column) = positions.at(span.start);
            let (end_line, end_column) = positions.at(span.end.max(span.start));
            return (
                ValidationReport {
                    valid: false,
                    errors: vec![Diagnostic {
                        severity: IssueSeverity::Error,
                        code: "invalid_toml".to_string(),
                        path: String::new(),
                        message: err.message().trim().to_string(),
                        hint: None,
                        line,
                        column,
                        end_line,
                        end_column,
                    }],
                    warnings: Vec::new(),
                },
                None,
            );
        }
    };

    let index = SpanIndex::build(text, &positions);
    let report = ValidationReport::from_issues(config.check(), &index);
    let parsed = report.valid.then_some(config);
    (report, parsed)
}

/// Byte offset -> (line, column), both 1-based.
struct Positions {
    /// Byte offset of the first character of every line.
    line_starts: Vec<usize>,
    text: String,
}

impl Positions {
    fn new(text: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self {
            line_starts,
            text: text.to_string(),
        }
    }

    fn at(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        // Columns are counted in characters: a byte offset would put the mark
        // in the wrong place on any line with non-ASCII text on it.
        let column = self.text[line_start..offset].chars().count();
        (line_index as u32 + 1, column as u32 + 1)
    }
}

/// Field path -> span, for every key in the document.
struct SpanIndex {
    spans: Vec<(String, Range<usize>)>,
    positions_line: Vec<u32>,
    positions_column: Vec<u32>,
    end_line: Vec<u32>,
    end_column: Vec<u32>,
}

impl SpanIndex {
    fn build(text: &str, positions: &Positions) -> Self {
        let mut spans: Vec<(String, Range<usize>)> = Vec::new();
        if let Ok(document) = ImDocument::parse(text.to_string()) {
            walk_table(document.as_table(), "", &mut spans);
        }
        spans.sort_by(|a, b| a.0.cmp(&b.0));

        let (mut positions_line, mut positions_column) = (Vec::new(), Vec::new());
        let (mut end_line, mut end_column) = (Vec::new(), Vec::new());
        for (_, span) in &spans {
            let (line, column) = positions.at(span.start);
            let (eline, ecolumn) = positions.at(span.end.max(span.start));
            positions_line.push(line);
            positions_column.push(column);
            end_line.push(eline);
            end_column.push(ecolumn);
        }

        Self {
            spans,
            positions_line,
            positions_column,
            end_line,
            end_column,
        }
    }

    fn find(&self, path: &str) -> Option<usize> {
        self.spans
            .binary_search_by(|(key, _)| key.as_str().cmp(path))
            .ok()
    }

    /// The position for a path, walking up to the parent table when the field
    /// itself is not in the file (an unset field still has a table to blame).
    fn resolve(&self, path: &str) -> Option<usize> {
        let mut candidate = path;
        loop {
            if let Some(index) = self.find(candidate) {
                return Some(index);
            }
            match candidate.rfind(['.', '[']) {
                Some(cut) if candidate.as_bytes()[cut] == b'.' => candidate = &candidate[..cut],
                // `route[2]` -> `route`: the array of tables is still better
                // than nothing when that exact element has no span.
                Some(cut) => candidate = &candidate[..cut],
                None => return None,
            }
        }
    }

    fn locate(&self, issue: ConfigIssue) -> Diagnostic {
        let found = self.resolve(&issue.path);
        Diagnostic {
            severity: issue.severity,
            code: issue.code.to_string(),
            path: issue.path,
            message: issue.message,
            hint: issue.hint,
            line: found.map_or(1, |index| self.positions_line[index]),
            column: found.map_or(1, |index| self.positions_column[index]),
            end_line: found.map_or(1, |index| self.end_line[index]),
            end_column: found.map_or(1, |index| self.end_column[index]),
        }
    }
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn walk_table(table: &Table, prefix: &str, out: &mut Vec<(String, Range<usize>)>) {
    for (key, item) in table.iter() {
        let path = join(prefix, key);
        // The key is what the mark should point at, not the whole table under
        // it: a 40-line service is not a useful underline.
        if let Some(span) = table.key(key).and_then(|key| key.span()) {
            out.push((path.clone(), span));
        }
        walk_item(item, &path, out);
    }
}

fn walk_item(item: &Item, path: &str, out: &mut Vec<(String, Range<usize>)>) {
    match item {
        Item::Table(table) => walk_table(table, path, out),
        Item::ArrayOfTables(array) => {
            for (index, table) in array.iter().enumerate() {
                let element = format!("{path}[{index}]");
                if let Some(span) = table.span() {
                    out.push((element.clone(), span));
                }
                walk_table(table, &element, out);
            }
        }
        Item::Value(value) => {
            if let Some(inline) = value.as_inline_table() {
                for (key, inline_value) in inline.iter() {
                    let inline_path = join(path, key);
                    if let Some(span) = inline.key(key).and_then(|key| key.span()) {
                        out.push((inline_path.clone(), span));
                    }
                    if let Some(span) = inline_value.span() {
                        out.push((format!("{inline_path}#value"), span));
                    }
                }
            }
            if let Some(array) = value.as_array() {
                for (index, element) in array.iter().enumerate() {
                    if let Some(inline) = element.as_inline_table() {
                        let element_path = format!("{path}[{index}]");
                        if let Some(span) = element.span() {
                            out.push((element_path.clone(), span));
                        }
                        for (key, inline_value) in inline.iter() {
                            let inline_path = join(&element_path, key);
                            if let Some(span) = inline.key(key).and_then(|key| key.span()) {
                                out.push((inline_path.clone(), span));
                            }
                            let _ = inline_value;
                        }
                    }
                }
            }
        }
        Item::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[server]
listen = ["0.0.0.0:8080"]

[[service]]
name = "api"
lb = "round_robin"

[[service.upstream]]
addr = "127.0.0.1:3001"
weight = 1

[[service.upstream]]
addr = ""
weight = 1

[[route]]
name = "api"
service = "nope"
path_prefix = "api"
is_default = true
"#;

    fn line_of(report: &ValidationReport, code: &str) -> u32 {
        report
            .errors
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .unwrap_or_else(|| panic!("no {code} in {:?}", report.errors))
            .line
    }

    #[test]
    fn reports_every_error_in_one_pass() {
        let (report, config) = validate_text(SAMPLE);

        assert!(!report.valid);
        assert!(config.is_none(), "an invalid config is not handed back");
        assert!(
            report.errors.len() >= 3,
            "expected the empty addr, the unknown service and the relative \
             path_prefix, got {:?}",
            report.errors
        );
    }

    #[test]
    fn points_at_the_line_the_field_is_on() {
        let (report, _) = validate_text(SAMPLE);

        // Line numbers are 1-based and the sample starts with a newline.
        assert_eq!(line_of(&report, "upstream_addr_empty"), 14);
        assert_eq!(line_of(&report, "unknown_service"), 19);
        assert_eq!(line_of(&report, "path_not_absolute"), 20);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let text = "# ทดสอบ\n[[route]]\nname = \"a\"\nservice = \"missing\"\npath_prefix = \"/\"\n";
        let (report, _) = validate_text(text);

        let diagnostic = report
            .errors
            .iter()
            .find(|diagnostic| diagnostic.code == "unknown_service")
            .expect("unknown service");
        assert_eq!((diagnostic.line, diagnostic.column), (4, 1));
    }

    #[test]
    fn an_unknown_service_lists_the_ones_that_exist() {
        let (report, _) = validate_text(SAMPLE);

        let hint = report
            .errors
            .iter()
            .find(|diagnostic| diagnostic.code == "unknown_service")
            .and_then(|diagnostic| diagnostic.hint.clone())
            .expect("a hint naming the services that do exist");
        assert!(hint.contains("api"), "{hint}");
    }

    #[test]
    fn a_syntax_error_is_reported_alone_and_in_place() {
        let (report, config) = validate_text("[server]\nlisten = [\n");

        assert!(config.is_none());
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "invalid_toml");
        assert!(report.errors[0].line >= 2, "{:?}", report.errors[0]);
    }

    #[test]
    fn a_valid_config_comes_back_parsed_with_its_warnings() {
        let text = r#"
[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3001"

[[service]]
name = "unused"

[[service.upstream]]
addr = "127.0.0.1:3002"

[[route]]
name = "api"
service = "api"
path_prefix = "/"
"#;
        let (report, config) = validate_text(text);

        assert!(report.valid, "{:?}", report.errors);
        assert!(config.is_some());
        let codes: Vec<&str> = report
            .warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect();
        assert!(codes.contains(&"unused_service"), "{codes:?}");
        assert!(codes.contains(&"no_default_route"), "{codes:?}");
    }

    #[test]
    fn a_warning_points_at_the_service_it_is_about() {
        let text = r#"
[[service]]
name = "api"

[[service.upstream]]
addr = "127.0.0.1:3001"

[[service]]
name = "unused"

[[service.upstream]]
addr = "127.0.0.1:3002"

[[route]]
name = "api"
service = "api"
path_prefix = "/"
is_default = true
"#;
        let (report, _) = validate_text(text);

        let warning = report
            .warnings
            .iter()
            .find(|warning| warning.code == "unused_service")
            .expect("unused service warning");
        assert_eq!(warning.line, 8, "{warning:?}");
    }
}
