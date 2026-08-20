//! `database-migrate` 的行值解码与类型转换。
//!
//! 这里负责把源库查询结果解码成统一的中间表示，再按目标列类型转换成
//! 可插入目标库的参数值。

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use sea_orm::{QueryResult, TryGetError, Value};

use crate::errors::{AsterError, Result};

use crate::cli::database_migration::helpers::{parse_bool, parse_timestamp};
use crate::cli::database_migration::{BindingKind, CellValue, TablePlan};

/// Decodes a source row and converts each cell into the target backend value shape.
pub(super) fn decode_row_values(
    row: &QueryResult,
    plan: &TablePlan,
    target_type_hints: &BTreeMap<String, BindingKind>,
) -> Result<Vec<Value>> {
    plan.columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let target_kind = target_type_hints
                .get(&column.name)
                .copied()
                .ok_or_else(|| {
                    AsterError::validation_error(format!(
                        "missing target type hint for {}.{}",
                        plan.name, column.name
                    ))
                })?;
            let cell = decode_source_cell(
                row,
                index,
                column.binding_kind,
                &column.raw_type,
                &plan.name,
                &column.name,
            )?;
            cell_into_target_value(cell, target_kind, &plan.name, &column.name)
        })
        .collect()
}

fn decode_source_cell(
    row: &QueryResult,
    index: usize,
    source_kind: BindingKind,
    raw_type: &str,
    table_name: &str,
    column_name: &str,
) -> Result<CellValue> {
    let decode_error = |error: TryGetError| {
        AsterError::database_operation(format!(
            "failed to decode {}.{} as '{}': {error:?}",
            table_name, column_name, raw_type
        ))
    };

    match source_kind {
        BindingKind::Bool => {
            if let Ok(value) = row.try_get_by_index_nullable::<Option<bool>>(index) {
                return Ok(value.map_or(CellValue::Null, CellValue::Bool));
            }
            let value = row
                .try_get_by_index_nullable::<Option<i32>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, |value| CellValue::Bool(value != 0)))
        }
        BindingKind::Int32 | BindingKind::Int64 => {
            if let Ok(value) = row.try_get_by_index_nullable::<Option<i64>>(index) {
                return Ok(value.map_or(CellValue::Null, CellValue::Int64));
            }
            if let Ok(value) = row.try_get_by_index_nullable::<Option<i32>>(index) {
                return Ok(
                    value.map_or(CellValue::Null, |value| CellValue::Int64(i64::from(value)))
                );
            }
            if let Ok(value) = row.try_get_by_index_nullable::<Option<u64>>(index) {
                return match value {
                    Some(value) => Ok(CellValue::Int64(i64::try_from(value).map_err(|_| {
                        AsterError::database_operation(format!(
                            "failed to decode {}.{} as '{}': u64 value {value} does not fit into i64",
                            table_name, column_name, raw_type
                        ))
                    })?)),
                    None => Ok(CellValue::Null),
                };
            }
            if let Ok(value) = row.try_get_by_index_nullable::<Option<u32>>(index) {
                return Ok(
                    value.map_or(CellValue::Null, |value| CellValue::Int64(i64::from(value)))
                );
            }
            let value = row
                .try_get_by_index_nullable::<Option<bool>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, |value| {
                CellValue::Int64(if value { 1 } else { 0 })
            }))
        }
        BindingKind::Float64 => {
            if let Ok(value) = row.try_get_by_index_nullable::<Option<f64>>(index) {
                return Ok(value.map_or(CellValue::Null, CellValue::Float64));
            }
            if let Ok(value) = row.try_get_by_index_nullable::<Option<f32>>(index) {
                return Ok(value.map_or(CellValue::Null, |value| CellValue::Float64(value as f64)));
            }
            let value = row
                .try_get_by_index_nullable::<Option<i64>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, |value| CellValue::Float64(value as f64)))
        }
        BindingKind::Bytes => {
            let value = row
                .try_get_by_index_nullable::<Option<Vec<u8>>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, CellValue::Bytes))
        }
        BindingKind::TimestampWithTimeZone => {
            let value = row
                .try_get_by_index_nullable::<Option<DateTime<FixedOffset>>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, CellValue::Timestamp))
        }
        BindingKind::Json => {
            if let Ok(value) = row.try_get_by_index_nullable::<Option<serde_json::Value>>(index) {
                return Ok(value.map_or(CellValue::Null, CellValue::Json));
            }
            let value = row
                .try_get_by_index_nullable::<Option<String>>(index)
                .map_err(decode_error)?;
            match value {
                Some(value) => {
                    let json = serde_json::from_str(&value).map_err(|error| {
                        AsterError::database_operation(format!(
                            "failed to parse {}.{} JSON text from '{}': {error}",
                            table_name, column_name, raw_type
                        ))
                    })?;
                    Ok(CellValue::Json(json))
                }
                None => Ok(CellValue::Null),
            }
        }
        BindingKind::String => {
            let value = row
                .try_get_by_index_nullable::<Option<String>>(index)
                .map_err(decode_error)?;
            Ok(value.map_or(CellValue::Null, CellValue::String))
        }
    }
}

fn cell_into_target_value(
    cell: CellValue,
    target_kind: BindingKind,
    table_name: &str,
    column_name: &str,
) -> Result<Value> {
    let conversion_error = |detail: &str| {
        AsterError::database_operation(format!(
            "failed to convert {}.{} for target binding: {detail}",
            table_name, column_name
        ))
    };

    Ok(match target_kind {
        BindingKind::Bool => match cell {
            CellValue::Null => Option::<bool>::None.into(),
            CellValue::Bool(value) => Some(value).into(),
            CellValue::Int64(value) => Some(value != 0).into(),
            CellValue::String(value) => Some(
                parse_bool(&value)
                    .ok_or_else(|| conversion_error("string value is not a valid boolean"))?,
            )
            .into(),
            _ => {
                return Err(conversion_error(
                    "unsupported source type for boolean target",
                ));
            }
        },
        BindingKind::Int32 => match cell {
            CellValue::Null => Option::<i32>::None.into(),
            CellValue::Bool(value) => Some(if value { 1 } else { 0 }).into(),
            CellValue::Int64(value) => Some(
                i32::try_from(value)
                    .map_err(|_| conversion_error("integer overflow while converting to i32"))?,
            )
            .into(),
            CellValue::String(value) => Some(
                value
                    .parse::<i32>()
                    .map_err(|_| conversion_error("string value is not a valid i32"))?,
            )
            .into(),
            _ => return Err(conversion_error("unsupported source type for int32 target")),
        },
        BindingKind::Int64 => match cell {
            CellValue::Null => Option::<i64>::None.into(),
            CellValue::Bool(value) => Some(if value { 1_i64 } else { 0_i64 }).into(),
            CellValue::Int64(value) => Some(value).into(),
            CellValue::String(value) => Some(
                value
                    .parse::<i64>()
                    .map_err(|_| conversion_error("string value is not a valid i64"))?,
            )
            .into(),
            _ => return Err(conversion_error("unsupported source type for int64 target")),
        },
        BindingKind::Float64 => match cell {
            CellValue::Null => Option::<f64>::None.into(),
            CellValue::Bool(value) => Some(if value { 1_f64 } else { 0_f64 }).into(),
            CellValue::Int64(value) => Some(value as f64).into(),
            CellValue::Float64(value) => Some(value).into(),
            CellValue::String(value) => Some(
                value
                    .parse::<f64>()
                    .map_err(|_| conversion_error("string value is not a valid f64"))?,
            )
            .into(),
            _ => return Err(conversion_error("unsupported source type for float target")),
        },
        BindingKind::String => match cell {
            CellValue::Null => Option::<String>::None.into(),
            CellValue::Bool(value) => Some(value.to_string()).into(),
            CellValue::Int64(value) => Some(value.to_string()).into(),
            CellValue::Float64(value) => Some(value.to_string()).into(),
            CellValue::Json(value) => Some(value.to_string()).into(),
            CellValue::String(value) => Some(value).into(),
            CellValue::Timestamp(value) => Some(value.to_rfc3339()).into(),
            CellValue::Bytes(_) => {
                return Err(conversion_error(
                    "cannot losslessly convert bytes into string",
                ));
            }
        },
        BindingKind::Json => match cell {
            CellValue::Null => Option::<serde_json::Value>::None.into(),
            CellValue::Json(value) => value.into(),
            CellValue::String(value) => serde_json::from_str::<serde_json::Value>(&value)
                .map_err(|_| conversion_error("string value is not valid JSON"))?
                .into(),
            _ => return Err(conversion_error("unsupported source type for JSON target")),
        },
        BindingKind::Bytes => match cell {
            CellValue::Null => Option::<Vec<u8>>::None.into(),
            CellValue::Bytes(value) => Some(value).into(),
            _ => return Err(conversion_error("unsupported source type for bytes target")),
        },
        BindingKind::TimestampWithTimeZone => match cell {
            CellValue::Null => Option::<DateTime<FixedOffset>>::None.into(),
            CellValue::Timestamp(value) => Some(value).into(),
            CellValue::String(value) => Some(parse_timestamp(&value).ok_or_else(|| {
                conversion_error("string value is not a valid RFC3339 timestamp")
            })?)
            .into(),
            _ => {
                return Err(conversion_error(
                    "unsupported source type for timestamp target",
                ));
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use sea_orm::Value;

    use super::{BindingKind, CellValue, cell_into_target_value};

    #[test]
    fn string_bool_cells_convert_into_bool_values() {
        let value = cell_into_target_value(
            CellValue::String("true".to_string()),
            BindingKind::Bool,
            "users",
            "enabled",
        )
        .expect("boolean conversion should succeed");

        assert_eq!(value, Value::from(Some(true)));
    }

    #[test]
    fn string_timestamp_cells_convert_into_timestamp_values() {
        let raw = "2026-04-15T12:34:56+08:00";
        let value = cell_into_target_value(
            CellValue::String(raw.to_string()),
            BindingKind::TimestampWithTimeZone,
            "audit_logs",
            "created_at",
        )
        .expect("timestamp conversion should succeed");
        let parsed = DateTime::parse_from_rfc3339(raw).expect("fixture timestamp should parse");

        assert_eq!(value, Value::from(Some(parsed)));
    }

    #[test]
    fn bytes_cannot_convert_into_string_values() {
        let error = cell_into_target_value(
            CellValue::Bytes(vec![1, 2, 3]),
            BindingKind::String,
            "file_blobs",
            "storage_path",
        )
        .expect_err("bytes to string conversion should fail");

        assert!(
            error
                .message()
                .contains("cannot losslessly convert bytes into string"),
            "unexpected error: {}",
            error.message()
        );
    }
}
