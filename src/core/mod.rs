use crate::{
	config::Config, info, repository::memory::SystemReportStore, services::broker::Broker,
};
use std::{thread, time::Duration};

pub mod models;

const BASE_SENSORS: &[&str] = &[
	"ram_total",
	"ram_usage",
	"disk_total",
	"disk_usage",
	"cpu_usage",
	"network_received",
	"network_transmitted",
];

const GPU_SENSORS: &[&str] = &["gpu_vendor", "gpu_mem_used", "gpu_mem_total", "gpu_usage"];

const PERCENT_SENSORS: &[&str] = &["cpu_usage", "gpu_usage"];
const RATE_SENSORS: &[&str] = &["network_received", "network_transmitted"];

pub struct SystemReporter {
	config: Config,
	store: SystemReportStore,
	broker: Broker,
	sensors: Vec<String>,
}

impl SystemReporter {
	pub fn new(
		config: Config,
		store: SystemReportStore,
		broker: Broker,
		gpu_vendor: Option<String>,
	) -> Self {
		let mut sensors: Vec<String> = BASE_SENSORS.iter().map(|s| s.to_string()).collect();
		if gpu_vendor.is_some() {
			sensors.extend(GPU_SENSORS.iter().map(|s| s.to_string()));
		}
		Self {
			config,
			store,
			broker,
			sensors,
		}
	}

	pub fn run(&self) {
		info!("reporting states...");
		let topic = format!("{}/{}/state", self.config.program_name, self.config.client_id);

		let mut refresh_count = 0u32;

		loop {
			if let Some(entry) = self.store.get_latest() {
				self.broker.publish(&topic, entry.report.to_string());
			}

			// Every 100 refreshes, reregister the sensors
			// this has been an issue with Home Assistant losing the sensor configs
			if refresh_count % 100 == 0 && refresh_count != 0 {
				self.register();
			}

			refresh_count += 1;
			thread::sleep(Duration::from_secs(self.config.report_interval));
		}
	}

	pub fn register(&self) {
		let topic = format!("{}/{}/state", self.config.program_name, self.config.client_id);
		let device_name = format!(
			"{} {}",
			self.config.program_name.replace("_", " "),
			self.config.client_id.replace("_", " ")
		);
		let model_id = format!("{}_{}", self.config.program_name, self.config.client_id);

		for sensor in &self.sensors {
			let config_topic =
				format!("homeassistant/sensor/{}_{}/config", self.config.client_id, sensor);

			let unique_id = format!("{}_{}", self.config.client_id, sensor);

			let value_template = format!("{{{{ value_json.{} }}}}", sensor);
			let config = if PERCENT_SENSORS.contains(&sensor.as_str()) {
				serde_json::json!({
					"state_topic": topic,
					"unit_of_measurement": "%",
					"value_template": value_template,
					"unique_id": unique_id,
					"name": sensor.replace("_", " "),
					"state_class": "measurement",
					"device": {
						"name": device_name,
						"identifiers": [model_id],
						"manufacturer": self.config.program_name,
						"model": model_id,
					}
				})
			} else if RATE_SENSORS.contains(&sensor.as_str()) {
				serde_json::json!({
					"device_class": "data_rate",
					"state_topic": topic,
					"unit_of_measurement": "MB/s",
					"value_template": value_template,
					"unique_id": unique_id,
					"name": sensor.replace("_", " "),
					"state_class": "measurement",
					"device": {
						"name": device_name,
						"identifiers": [model_id],
						"manufacturer": self.config.program_name,
						"model": model_id,
					}
				})
			} else if sensor == "gpu_vendor" {
				serde_json::json!({
					"state_topic": topic,
					"value_template": value_template,
					"unique_id": unique_id,
					"name": sensor.replace("_", " "),
					"device": {
						"name": device_name,
						"identifiers": [model_id],
						"manufacturer": self.config.program_name,
						"model": model_id,
					}
				})
			} else {
				let unit = if sensor.contains("ram") || sensor.starts_with("gpu_mem") {
					self.config.memory_unit.to_string()
				} else {
					self.config.storage_unit.to_string()
				};

				serde_json::json!({
					"device_class": "data_size",
					"state_topic": topic,
					"unit_of_measurement": unit,
					"value_template": value_template,
					"unique_id": unique_id,
					"name": sensor.replace("_", " "),
					"state_class": "total",
					"device": {
						"name": device_name,
						"identifiers": [model_id],
						"manufacturer": self.config.program_name,
						"model": model_id,
					}
				})
			};

			info!("registered {}...", sensor);

			self.broker.publish(&config_topic, config.to_string());
		}
	}
}
