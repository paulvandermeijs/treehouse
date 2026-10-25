//! Adjustments to Pi's Markdown before it is shown.

/// Right-aligns table columns whose values are all numbers, dates, sizes or
/// durations, as the design asks. Columns with their own alignment marker
/// keep it; code blocks are left alone.
pub fn align_numeric_columns(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut in_fence = false;
    let mut ix = 0;
    while ix < lines.len() {
        let trimmed = lines[ix].trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            ix += 1;
            continue;
        }
        if in_fence || ix + 1 >= lines.len() || !lines[ix].contains('|') {
            ix += 1;
            continue;
        }
        let Some(delimiters) = delimiter_row(lines[ix + 1]) else {
            ix += 1;
            continue;
        };
        let body: Vec<Vec<String>> = lines[ix + 2..]
            .iter()
            .take_while(|l| l.contains('|') && !l.trim().is_empty())
            .map(|l| cells(l))
            .collect();
        let aligned: Vec<String> = delimiters
            .iter()
            .enumerate()
            .map(|(col, delimiter)| {
                let numeric = !delimiter.contains(':')
                    && body
                        .iter()
                        .any(|row| row.get(col).is_some_and(|c| !c.is_empty()))
                    && body
                        .iter()
                        .all(|row| row.get(col).is_none_or(|c| c.is_empty() || is_numeric(c)));
                if numeric {
                    "---:".to_string()
                } else {
                    delimiter.clone()
                }
            })
            .collect();
        if aligned != delimiters {
            out[ix + 1] = format!("| {} |", aligned.join(" | "));
        }
        ix += 2 + body.len();
    }
    out.join("\n")
}

/// The cells of a delimiter row (`| --- | :--: |`), or `None` for any other line.
fn delimiter_row(line: &str) -> Option<Vec<String>> {
    let cells = cells(line);
    let is_delimiter = !cells.is_empty()
        && cells.iter().all(|c| {
            let c = c.trim_matches(':');
            !c.is_empty() && c.chars().all(|ch| ch == '-')
        });
    is_delimiter.then_some(cells)
}

/// A table row's cells, trimmed, without the outer pipes. `\|` doesn't split.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    let mut cells = vec![String::new()];
    let mut chars = line.chars().peekable();
    let mut in_code = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                cells.last_mut().unwrap().push_str("\\|");
                chars.next();
            }
            '`' => {
                in_code = !in_code;
                cells.last_mut().unwrap().push(ch);
            }
            '|' if !in_code => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(ch),
        }
    }
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

/// Numbers with an optional sign, currency or unit ("−2.31 ms", "96 KB",
/// "+12.5%", "$1,200"), and dates or times ("2025-02-17", "3:05").
fn is_numeric(cell: &str) -> bool {
    let cell = cell
        .trim_matches(|c| c == '*' || c == '`' || c == '_')
        .trim();
    let rest = cell.trim_start_matches(['+', '-', '−', '±', '~', '≈']);
    let rest = rest.trim_start_matches(['$', '€', '£', '¥']);
    let Some(first) = rest.chars().next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    let number_end = rest
        .find(|c: char| !(c.is_ascii_digit() || ",._:-/".contains(c)))
        .unwrap_or(rest.len());
    let unit = rest[number_end..].trim();
    unit.is_empty()
        || unit == "%"
        || unit.chars().count() <= 5 && unit.chars().all(|c| c.is_alphabetic() || c == '/')
}

#[cfg(test)]
mod tests {
    use super::{align_numeric_columns, is_numeric};

    #[test]
    fn right_aligns_numeric_columns() {
        let source = "Compare:\n\n| Crate | Size | Released |\n| --- | --- | :-- |\n| `similar` | 96 KB | 2024-11-03 |\n| diffy | 41 KB | 2023-08-29 |\n\nDone.";
        let aligned = align_numeric_columns(source);
        // Size becomes right-aligned; Released keeps its own marker.
        assert!(aligned.contains("| --- | ---: | :-- |"), "{aligned}");
        assert!(aligned.ends_with("Done."));
    }

    #[test]
    fn leaves_code_and_text_columns() {
        let fenced = "```\n| a | b |\n| - | - |\n| 1 | 2 |\n```";
        assert_eq!(align_numeric_columns(fenced), fenced);
        let text = "| Name | Note |\n| --- | --- |\n| a | 3 items left |";
        assert_eq!(align_numeric_columns(text), text);
    }

    #[test]
    fn recognises_numbers() {
        for yes in [
            "4.82 ms",
            "−2.31 ms",
            "+12.5%",
            "$1,200",
            "2025-02-17",
            "3:05",
            "100",
            "**42**",
        ] {
            assert!(is_numeric(yes), "{yes}");
        }
        for no in ["similar", "v1.2", "Myers", "3 items left", ""] {
            assert!(!is_numeric(no), "{no}");
        }
    }
}
