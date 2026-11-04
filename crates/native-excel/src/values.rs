//! Column-aware cell conversion and formatting.
//!
//! Uniform styles are stored in column metadata. Rules and cell overrides pass
//! explicit formats when writing individual cells.
use crate::{
    BinaryEncoding, Column, ColumnTransform, DataType, ExportError, PrecisionPolicy, Result,
};
use chrono::Datelike;
use rust_xlsxwriter::{Format, IntoExcelDateTime, Worksheet};
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
                        Some(
                            "int64"
                                | "decimal"
                                | "date"
                                | "timestamp"
                                | "json"
                                | "formula"
                                | "binary"
                        )
                    )
                })
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
        })
        .ok_or_else(mismatch)
}

fn apply_transforms(input: &str, transforms: &[ColumnTransform]) -> String {
    let mut current = input.to_owned();
    for transform in transforms {
        current = match transform {
            ColumnTransform::Trim => current.trim().to_owned(),
            ColumnTransform::Uppercase => current.to_uppercase(),
            ColumnTransform::Lowercase => current.to_lowercase(),
        };
    }
    current
}

fn write_text(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    text: &str,
    format: Option<&Format>,
) -> Result<()> {
    if text.chars().count() > 32767 {
        return Err(ExportError::new(
            "LIMIT_EXCEEDED",
            "Cell text exceeds Excel limit",
        ));
    }
    if let Some(fmt) = format {
        ws.write_string_with_format(row, col, text, fmt)?;
    } else {
        ws.write_string(row, col, text)?;
    }
    Ok(())
}

fn write_number(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: f64,
    format: Option<&Format>,
) -> Result<()> {
    if !value.is_finite() {
        return Err(mismatch());
    }
    if let Some(fmt) = format {
        ws.write_number_with_format(row, col, value, fmt)?;
    } else {
        ws.write_number(row, col, value)?;
    }
    Ok(())
}

fn write_bool(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: bool,
    format: Option<&Format>,
) -> Result<()> {
    if let Some(fmt) = format {
        ws.write_boolean_with_format(row, col, value, fmt)?;
    } else {
        ws.write_boolean(row, col, value)?;
    }
    Ok(())
}

fn write_formula(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    formula: &str,
    format: Option<&Format>,
) -> Result<()> {
    if let Some(fmt) = format {
        ws.write_formula_with_format(row, col, formula, fmt)?;
    } else {
        ws.write_formula(row, col, formula)?;
    }
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

pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return Err(mismatch());
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for pair in bytes.as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair).map_err(|_| mismatch())?;
        let byte = u8::from_str_radix(s, 16).map_err(|_| mismatch())?;
        out.push(byte);
    }
    Ok(out)
}

fn decode_base64(input: &str) -> Result<Vec<u8>> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return Err(mismatch());
    }
    let val = |b: u8| -> Result<u8> {
        match b {
            b'A'..=b'Z' => Ok(b - b'A'),
            b'a'..=b'z' => Ok(b - b'a' + 26),
            b'0'..=b'9' => Ok(b - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(mismatch()),
        }
    };
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.as_chunks::<4>().0 {
        let v0 = u32::from(val(chunk[0])?);
        let v1 = u32::from(val(chunk[1])?);
        let v2 = if chunk[2] == b'=' {
            0
        } else {
            u32::from(val(chunk[2])?)
        };
        let v3 = if chunk[3] == b'=' {
            0
        } else {
            u32::from(val(chunk[3])?)
        };
        let triple = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}

fn extract_binary_bytes(value: &Value) -> Result<Vec<u8>> {
    match value {
        Value::Array(arr) => {
            let mut bytes = Vec::with_capacity(arr.len());
            for item in arr {
                let n = item.as_u64().ok_or_else(mismatch)?;
                let b = u8::try_from(n).map_err(|_| mismatch())?;
                bytes.push(b);
            }
            Ok(bytes)
        }
        Value::Object(obj) => {
            if obj.get("$type").and_then(Value::as_str) != Some("binary") {
                return Err(mismatch());
            }
            let raw = obj
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(mismatch)?;
            match obj
                .get("encoding")
                .and_then(Value::as_str)
                .unwrap_or("base64")
            {
                "hex" => decode_hex(raw),
                "base64" => decode_base64(raw),
                _ => Err(mismatch()),
            }
        }
        Value::String(s) => decode_hex(s).or_else(|_| decode_base64(s)),
        _ => Err(mismatch()),
    }
}

fn parse_fixed_offset(tz: &str) -> Result<chrono::FixedOffset> {
    if tz.eq_ignore_ascii_case("utc") || tz == "Z" {
        return Ok(chrono::FixedOffset::east_opt(0).unwrap());
    }
    let sign = match tz.as_bytes().first() {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => {
            return Err(ExportError::invalid(
                "Invalid timezone offset; expected UTC or +HH:MM / -HH:MM",
            ));
        }
    };
    let rest = &tz[1..];
    let (hh, mm) = rest.split_once(':').ok_or_else(|| {
        ExportError::invalid("Invalid timezone offset; expected +HH:MM or -HH:MM")
    })?;
    let hours: i32 = hh
        .parse()
        .map_err(|_| ExportError::invalid("Invalid timezone hours"))?;
    let mins: i32 = mm
        .parse()
        .map_err(|_| ExportError::invalid("Invalid timezone minutes"))?;
    if !(0..=14).contains(&hours) || !(0..=59).contains(&mins) {
        return Err(ExportError::invalid("Timezone offset out of range"));
    }
    let total_secs = sign * (hours * 3600 + mins * 60);
    chrono::FixedOffset::east_opt(total_secs)
        .ok_or_else(|| ExportError::invalid("Invalid timezone offset"))
}

pub(crate) fn write_value(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: &Value,
    kind: DataType,
) -> Result<()> {
    write_value_with_format(ws, row, col, value, kind, None, &Column::new("", kind))
}

pub(crate) fn write_value_with_format(
    ws: &mut Worksheet,
    row: u32,
    col: u16,
    value: &Value,
    kind: DataType,
    format: Option<&Format>,
    col_spec: &Column,
) -> Result<()> {
    if value.is_null() {
        if let Some(placeholder) = &col_spec.null_placeholder {
            return write_text(ws, row, col, placeholder, format);
        }
        if let Some(fmt) = format {
            ws.write_blank(row, col, fmt)?;
        }
        return Ok(());
    }
    match kind {
        DataType::Auto => match value {
            Value::String(s) => {
                let transformed = apply_transforms(s, &col_spec.transforms);
                write_text(ws, row, col, &transformed, format)
            }
            Value::Bool(v) => write_bool(ws, row, col, *v, format),
            Value::Number(v) => {
                if v.as_i64()
                    .is_some_and(|v| v.unsigned_abs() > 999_999_999_999_999)
                    || v.as_u64().is_some_and(|v| v > 999_999_999_999_999)
                {
                    write_text(ws, row, col, &v.to_string(), format)
                } else {
                    write_number(ws, row, col, v.as_f64().ok_or_else(mismatch)?, format)
                }
            }
            Value::Object(v) => match v.get("$type").and_then(Value::as_str) {
                Some("decimal" | "int64" | "date" | "timestamp" | "json") => {
                    let s = apply_transforms(text(value)?, &col_spec.transforms);
                    write_text(ws, row, col, &s, format)
                }
                Some("formula") => write_formula(ws, row, col, text(value)?, format),
                _ => Err(mismatch()),
            },
            _ => Err(mismatch()),
        },
        DataType::Text => {
            let raw = match value {
                Value::Number(_) | Value::Bool(_) => value.to_string(),
                _ => text(value)?.to_owned(),
            };
            let transformed = apply_transforms(&raw, &col_spec.transforms);
            write_text(ws, row, col, &transformed, format)
        }
        DataType::Boolean => {
            write_bool(ws, row, col, value.as_bool().ok_or_else(mismatch)?, format)
        }
        DataType::Number => write_number(
            ws,
            row,
            col,
            match value {
                Value::Number(n) => n.as_f64().ok_or_else(mismatch)?,
                _ => text(value)?.parse::<f64>().map_err(|_| mismatch())?,
            },
            format,
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
                match col_spec.precision_policy {
                    PrecisionPolicy::ExactText => write_text(ws, row, col, &rendered, format),
                    PrecisionPolicy::Numeric | PrecisionPolicy::Round => {
                        write_number(ws, row, col, v as f64, format)
                    }
                }
            } else {
                write_number(ws, row, col, v as f64, format)
            }
        }
        DataType::Decimal => {
            let rendered = match value {
                Value::String(s) => s.clone(),
                Value::Object(_) => text(value)?.to_owned(),
                Value::Number(n) if col_spec.precision_policy != PrecisionPolicy::ExactText => {
                    n.to_string()
                }
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
            match col_spec.precision_policy {
                PrecisionPolicy::ExactText => write_text(ws, row, col, &rendered, format),
                PrecisionPolicy::Numeric => {
                    let dec: rust_decimal::Decimal = rendered.parse().map_err(|_| mismatch())?;
                    let float_val: f64 = dec.to_string().parse().map_err(|_| mismatch())?;
                    write_number(ws, row, col, float_val, format)
                }
                PrecisionPolicy::Round => {
                    let dec: rust_decimal::Decimal = rendered.parse().map_err(|_| mismatch())?;
                    let scale = col_spec.round_scale.unwrap_or(2);
                    let rounded = dec.round_dp(scale);
                    let float_val: f64 = rounded.to_string().parse().map_err(|_| mismatch())?;
                    write_number(ws, row, col, float_val, format)
                }
            }
        }
        DataType::Date => {
            let date = chrono::NaiveDate::parse_from_str(text(value)?, "%Y-%m-%d")
                .map_err(|_| mismatch())?;
            date_in_range(date.year())?;
            write_number(ws, row, col, date.to_excel_serial_date(), format)
        }
        DataType::Datetime => {
            let s = text(value)?;
            if col_spec
                .timezone
                .as_deref()
                .is_some_and(|tz| tz.eq_ignore_ascii_case("preserveText"))
            {
                let _ = chrono::DateTime::parse_from_rfc3339(s)
                    .map(|d| d.naive_utc())
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
                    .map_err(|_| mismatch())?;
                return write_text(ws, row, col, s, format);
            }
            let dt = if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(s) {
                if let Some(tz) = &col_spec.timezone {
                    let offset = parse_fixed_offset(tz)?;
                    parsed.with_timezone(&offset).naive_local()
                } else {
                    parsed.naive_utc()
                }
            } else {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
                    .map_err(|_| mismatch())?
            };
            date_in_range(dt.year())?;
            write_number(ws, row, col, dt.to_excel_serial_date(), format)
        }
        DataType::Time => {
            let time = chrono::NaiveTime::parse_from_str(text(value)?, "%H:%M:%S%.f")
                .map_err(|_| mismatch())?;
            write_number(ws, row, col, time.to_excel_serial_date(), format)
        }
        DataType::Formula => write_formula(ws, row, col, text(value)?, format),
        DataType::Binary => {
            let encoding = col_spec.binary_encoding.ok_or_else(|| {
                ExportError::new(
                    "CONVERSION_FAILED",
                    "Binary columns require explicit binaryEncoding (hex or base64)",
                )
            })?;
            let bytes = extract_binary_bytes(value)?;
            let encoded = match encoding {
                BinaryEncoding::Hex => bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                BinaryEncoding::Base64 => encode_base64(&bytes),
            };
            write_text(ws, row, col, &encoded, format)
        }
    }
}
