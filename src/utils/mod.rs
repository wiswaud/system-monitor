use crate::models::{ByteInfo, ByteUnit};

pub mod macros;

pub fn format_bytes(bytes: u64, byte_unit: ByteUnit) -> ByteInfo {
	format_bytes_with_precision(bytes, byte_unit, 2)
}

pub fn format_bytes_with_precision(bytes: u64, byte_unit: ByteUnit, precision: usize) -> ByteInfo {
	let raw_value = bytes as f64 / byte_unit.to_bytes();
	let multiplier = 10_f64.powi(precision as i32);
	let rounded_value = (raw_value * multiplier).round() / multiplier;

	ByteInfo {
		value: rounded_value,
		unit: byte_unit.to_string(),
		precision,
	}
}

pub fn format_rate(bytes_per_second: u64, byte_unit: ByteUnit) -> ByteInfo {
	format_rate_with_precision(bytes_per_second, byte_unit, 2)
}

pub fn format_rate_with_precision(
	bytes_per_second: u64,
	byte_unit: ByteUnit,
	precision: usize,
) -> ByteInfo {
	let raw_value = bytes_per_second as f64 / byte_unit.to_bytes();
	let multiplier = 10_f64.powi(precision as i32);
	let rounded_value = (raw_value * multiplier).round() / multiplier;

	ByteInfo {
		value: rounded_value,
		unit: format!("{}/s", byte_unit),
		precision,
	}
}
