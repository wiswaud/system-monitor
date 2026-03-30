use std::fmt;

#[derive(Debug, Clone)]
pub struct SystemReport {
	pub ram_total: ByteInfo,
	pub ram_usage: ByteInfo,
	pub disk_total: ByteInfo,
	pub disk_usage: ByteInfo,
	pub cpu_usage: String,
	pub network_received: ByteInfo,
	pub network_transmitted: ByteInfo,
	pub gpu_vendor: Option<String>,
	pub gpu_mem_used: Option<ByteInfo>,
	pub gpu_mem_total: Option<ByteInfo>,
	pub gpu_usage: Option<f64>,
}

impl fmt::Display for SystemReport {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let mut json = format!(
			r#"{{
"ram_total": {:.prec_ram_total$},
"ram_usage": {:.prec_ram_usage$},
"disk_total": {:.prec_disk_total$},
"disk_usage": {:.prec_disk_usage$},
"cpu_usage": {},
"network_received": {:.prec_network_received$},
"network_transmitted": {:.prec_network_transmitted$}"#,
			self.ram_total.value,
			self.ram_usage.value,
			self.disk_total.value,
			self.disk_usage.value,
			self.cpu_usage,
			self.network_received.value,
			self.network_transmitted.value,
			prec_ram_total = self.ram_total.precision,
			prec_ram_usage = self.ram_usage.precision,
			prec_disk_total = self.disk_total.precision,
			prec_disk_usage = self.disk_usage.precision,
			prec_network_received = self.network_received.precision,
			prec_network_transmitted = self.network_transmitted.precision,
		);

		if let Some(ref vendor) = self.gpu_vendor {
			json.push_str(&format!(r#","gpu_vendor": "{}""#, vendor));
		}
		if let (Some(used), Some(total)) = (&self.gpu_mem_used, &self.gpu_mem_total) {
			json.push_str(&format!(
				r#","gpu_mem_used": {:.prec$},"gpu_mem_total": {:.prec$}"#,
				used.value,
				total.value,
				prec = used.precision,
			));
		}
		if let Some(gpu_usage) = self.gpu_usage {
			json.push_str(&format!(r#","gpu_usage": {:.1}"#, gpu_usage));
		}

		json.push_str("\n}");
		write!(f, "{}", json)
	}
}

// let config = serde_json::json!({
// 	"device_class": "data_size",
// 	"state_topic": topic,
// 	"unit_of_measurement": self.config.byte_unit.to_string(),
// 	"value_template": value_template,
// 	"unique_id": sensor,
// 	"name": sensor.replace("_", " "),
// 	"state_class": "total",
// 	"device": {
// 		"name": device_name,
// 		"identifiers": [model_id],
// 		"manufacturer": self.config.program_name,
// 		"model": model_id,
// 	}
// });

#[derive(Debug, Clone)]
pub struct EntityConfig {
	pub device_class: DeviceClass,
}

#[derive(Debug, Clone)]
pub enum DeviceClass {
	DataSize,
}

#[derive(Debug, Clone)]
pub struct ByteInfo {
	pub value: f64,
	pub unit: String,
	pub precision: usize,
}

#[derive(Debug, Clone)]
pub enum ByteUnit {
	Byte,
	Kilobyte,
	Megabyte,
	Gigabyte,
	Terabyte,
	Petabyte,
}

impl ByteUnit {
	pub fn parse(unit: &str) -> ByteUnit {
		match unit {
			"KB" => ByteUnit::Kilobyte,
			"MB" => ByteUnit::Megabyte,
			"GB" => ByteUnit::Gigabyte,
			"TB" => ByteUnit::Terabyte,
			"PB" => ByteUnit::Petabyte,
			_ => ByteUnit::Byte,
		}
	}

	pub fn to_bytes(&self) -> f64 {
		match self {
			ByteUnit::Byte => 1.0,
			ByteUnit::Kilobyte => 1024.0,
			ByteUnit::Megabyte => 1024.0 * 1024.0,
			ByteUnit::Gigabyte => 1024.0 * 1024.0 * 1024.0,
			ByteUnit::Terabyte => 1024.0 * 1024.0 * 1024.0 * 1024.0,
			ByteUnit::Petabyte => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
		}
	}
}

impl std::fmt::Display for ByteUnit {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let unit_str = match self {
			ByteUnit::Byte => "B",
			ByteUnit::Kilobyte => "KB",
			ByteUnit::Megabyte => "MB",
			ByteUnit::Gigabyte => "GB",
			ByteUnit::Terabyte => "TB",
			ByteUnit::Petabyte => "PB",
		};
		write!(f, "{}", unit_str)
	}
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
	pub unique_id: String,
	pub name: String,
	pub state_topic: String,
	pub state_class: String,
	pub device_class: String,
	pub unit_of_measurement: String,
	pub device: Device,
}

#[derive(Debug, Clone)]
pub struct Device {
	pub identifiers: Vec<String>,
	pub manufacturer: String,
	pub model: String,
	pub name: String,
}
