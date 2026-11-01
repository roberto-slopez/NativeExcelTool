//! Column-aware conversion. Uniform styles live in column metadata, not this loop.
use crate::{DataType, ExportError, Result};
use chrono::Datelike;
use rust_xlsxwriter::{IntoExcelDateTime, Worksheet};
use serde_json::Value;

fn mismatch() -> ExportError {
    ExportError::new(
        "CONVERSION_FAILED",
        "Value does not match the declared column dataType",
    )
}
fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .or_else(|| {
            value
                .as_object()
                .filter(|v| {
                    matches!(
                        v.get("$type").and_then(Value::as_str),
                        Some("int64" | "decimal" | "date" | "timestamp" | "json" | "formula")
                    )
                })
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
        })
        .ok_or_else(mismatch)
}
fn write_text(ws: &mut Worksheet, row: u32, col: u16, text: &str) -> Result<()> {
    if text.chars().count() > 32767 {
        return Err(ExportError::new(
            "LIMIT_EXCEEDED",
            "Cell text exceeds Excel limit",
        ));
    }
    ws.write_string(row, col, text)?;
    Ok(())
}
fn write_number(ws: &mut Worksheet, row: u32, col: u16, value: f64) -> Result<()> {
    if !value.is_finite() {
        return Err(mismatch());
    }
    ws.write_number(row, col, value)?;
    Ok(())
}
fn date_in_range(year: i32) -> Result<()> {
    if !(1900..=9999).contains(&year) {
        Err(ExportError::new(
            "CONVERSION_FAILED",
            "Excel date year must be 1900..9999",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn write_value(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: &Value,
    kind: DataType,
) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    match kind {
        DataType::Auto => match value {
            Value::String(s) => write_text(ws, row, col, s),
            Value::Bool(v) => {
                ws.write_boolean(row, col, *v)?;
                Ok(())
            }
            Value::Number(v) => {
                if v.as_i64()
                    .is_some_and(|v| v.unsigned_abs() > 999_999_999_999_999)
                    || v.as_u64().is_some_and(|v| v > 999_999_999_999_999)
                {
                    write_text(ws, row, col, &v.to_string())
                } else {
                    write_number(ws, row, col, v.as_f64().ok_or_else(mismatch)?)
                }
            }
            Value::Object(v) => match v.get("$type").and_then(Value::as_str) {
                Some("decimal" | "int64" | "date" | "timestamp" | "json") => {
                    write_text(ws, row, col, text(value)?)
                }
                Some("formula") => {
                    ws.write_formula(row, col, text(value)?)?;
                    Ok(())
                }
                _ => Err(mismatch()),
            },
            _ => Err(mismatch()),
        },
        DataType::Text => match value {
            Value::Number(_) | Value::Bool(_) => write_text(ws, row, col, &value.to_string()),
            _ => write_text(ws, row, col, text(value)?),
        },
        DataType::Boolean => {
            ws.write_boolean(row, col, value.as_bool().ok_or_else(mismatch)?)?;
            Ok(())
        }
        DataType::Number => write_number(
            ws,
            row,
            col,
            match value {
                Value::Number(n) => n.as_f64().ok_or_else(mismatch)?,
                _ => text(value)?.parse::<f64>().map_err(|_| mismatch())?,
            },
        ),
        DataType::Integer => {
            let rendered = match value {
                Value::Number(n) => {
                    if n.as_i64().is_none() && n.as_u64().is_none() {
                        return Err(mismatch());
                    }
                    n.to_string()
                }
                _ => text(value)?.to_owned(),
            };
            let v = rendered.parse::<i128>().map_err(|_| mismatch())?;
            if v.unsigned_abs() > 999_999_999_999_999 {
                write_text(ws, row, col, &rendered)
            } else {
                write_number(ws, row, col, v as f64)
            }
        }
        DataType::Decimal => {
            let rendered = match value {
                Value::String(s) => s.clone(),
                Value::Object(_) => text(value)?.to_owned(),
                _ => {
                    return Err(ExportError::new(
                        "CONVERSION_FAILED",
                        "Exact decimals require text or a decimal tag; use number for Excel numeric currency",
                    ));
                }
            };
            let digits = rendered
                .strip_prefix('-')
                .or_else(|| rendered.strip_prefix('+'))
                .unwrap_or(&rendered);
            if digits.is_empty()
                || !digits.chars().any(|c| c.is_ascii_digit())
                || digits.chars().filter(|c| *c == '.').count() > 1
                || !digits.chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                return Err(mismatch());
            }
            write_text(ws, row, col, &rendered)
        }
        DataType::Date => {
            let date = chrono::NaiveDate::parse_from_str(text(value)?, "%Y-%m-%d")
                .map_err(|_| mismatch())?;
            date_in_range(date.year())?;
            write_number(ws, row, col, date.to_excel_serial_date())
        }
        DataType::Datetime => {
            let s = text(value)?;
            let dt = chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.naive_utc())
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
                .map_err(|_| mismatch())?;
            date_in_range(dt.year())?;
            write_number(ws, row, col, dt.to_excel_serial_date())
        }
        DataType::Time => {
            let time = chrono::NaiveTime::parse_from_str(text(value)?, "%H:%M:%S%.f")
                .map_err(|_| mismatch())?;
            write_number(ws, row, col, time.to_excel_serial_date())
        }
        DataType::Formula => {
            ws.write_formula(row, col, text(value)?)?;
            Ok(())
        }
    }
}
