//! The challenge's deliberately restricted CSV dialect.

/// Parse a single line without its `\n`. A trailing `\r` is data.
/// Returns `None` for malformed quoting or the wrong number of fields.
pub fn parse_line(line: &str, fields: usize) -> Option<Vec<&str>> {
    if line.is_empty() { return None; }
    let mut result = Vec::with_capacity(fields);
    for field in line.split(',') {
        let value = if field.starts_with('"') {
            if field.len() < 2 || !field.ends_with('"') { return None; }
            let value = &field[1..field.len()-1];
            if value.contains('"') { return None; }
            value
        } else {
            if field.contains('"') { return None; }
            field
        };
        result.push(value);
    }
    (result.len() == fields).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn quotes_and_empty() {
        assert_eq!(parse_line("\"u1\",\"\", a", 3), Some(vec!["u1", "", " a"]));
        assert_eq!(parse_line("a,b\r", 2), Some(vec!["a", "b\r"]));
    }
    #[test] fn malformed() {
        for line in ["", "a", "a,b,c", "a,\"b", "a,b\"", "a,\"b\"x", "a,\"b\"\"c\""] {
            assert_eq!(parse_line(line, 2), None, "{line:?}");
        }
    }
}
