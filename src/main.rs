use crate::{
	config::Config,
	core::SystemReporter,
	services::{broker::Broker, system_monitor::SystemMonitor},
};
pub use core::models;
pub use error::{Error, Result};
use repository::memory::SystemReportStore;

mod config;
mod core;
mod error;
mod repository;
mod services;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
	info!("initializing...");

	let config = Config::new()?;
	let store = SystemReportStore::new();

	let producer_store = store.clone();
	let monitor = SystemMonitor::new(config.clone(), producer_store);

	if !monitor.check_support() {
		return Err(Error::UnsupportedOS);
	}

	let broker = Broker::new(config.clone());

	let result = broker.wait_until_ready();

	match result {
		Ok(_) => {
			let gpu_vendor = monitor.gpu_vendor();
			let reporter = SystemReporter::new(config, store, broker, gpu_vendor);

			monitor.run();

			reporter.register();
			reporter.run();
		}
		Err(_) => {
			error!("Failed to connect to broker");
			return Err(Error::MainLoopClosed);
		}
	}

	Err(Error::MainLoopClosed)
}
