use super::{message, russian_locale};
use std::path::Path;

/// A TOML value, or the text as a string when it is not one (a bare path, for example).
pub(super) fn toml_value(text: &str) -> toml::Value {
    format!("v = {text}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(text.to_owned()))
}

pub(super) fn safe_toml_error(path: &Path, source: &str, error: &toml::de::Error) -> anyhow::Error {
    let location = error
        .span()
        .map(|span| {
            let prefix = source
                .as_bytes()
                .get(..span.start.min(source.len()))
                .unwrap_or_default();
            let line = prefix.split(|byte| *byte == b'\n').count();
            let column = prefix
                .iter()
                .rev()
                .take_while(|&&byte| byte != b'\n')
                .count()
                .saturating_add(1);
            if russian_locale() {
                format!(" в строке {line}, столбце {column}")
            } else {
                format!(" at line {line}, column {column}")
            }
        })
        .unwrap_or_default();
    // Display includes the source line, which can hold turn.secret.
    anyhow::anyhow!(
        "config {}: {}{location}: {}",
        path.display(),
        message("TOML parse error", "ошибка синтаксиса TOML"),
        error.message()
    )
}

/// Find a known field's type mismatch without formatting its potentially secret value.
pub(super) fn first_type_mismatch(
    defaults: &toml::Value,
    values: &toml::Table,
) -> Option<(String, &'static str)> {
    fn find(
        defaults: &toml::Table,
        values: &toml::Table,
        prefix: &str,
    ) -> Option<(String, &'static str)> {
        for (key, expected) in defaults {
            let Some(actual) = values.get(key) else {
                continue;
            };
            let name = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if !expected.same_type(actual) {
                return Some((name, expected.type_str()));
            }
            if let (Some(expected), Some(actual)) = (expected.as_table(), actual.as_table()) {
                if let Some(mismatch) = find(expected, actual, &name) {
                    return Some(mismatch);
                }
            }
        }
        None
    }
    find(defaults.as_table()?, values, "")
}

pub(super) fn expected_type(english: &'static str) -> &'static str {
    match english {
        "string" => message("string", "строка"),
        "integer" => message("integer", "целое число"),
        "float" => message("float", "дробное число"),
        "boolean" => message("boolean", "логическое значение"),
        "array" => message("array", "список"),
        "table" => message("table", "раздел"),
        "datetime" => message("datetime", "дата и время"),
        _ => english,
    }
}

/// Sets `path` in `table`, creating the sections on the way; `name` labels errors.
pub(super) fn set(
    table: &mut toml::Table,
    path: &[String],
    value: toml::Value,
    name: &str,
) -> anyhow::Result<()> {
    let (last, sections) = path
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("{name}: empty configuration path"))?;
    let mut node = table;
    for section in sections {
        node = node
            .entry(section.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{name}: {section} {}",
                    message("is not a section", "не является разделом")
                )
            })?;
    }
    node.insert(last.clone(), value);
    Ok(())
}
