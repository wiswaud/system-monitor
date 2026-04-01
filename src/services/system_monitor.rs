use crate::{
	config::Config,
	info,
	warn,
	models::{ByteInfo, SystemReport},
	repository::memory::SystemReportStore,
	services::gpu_monitor::GpuMonitor,
	utils::{format_bytes, format_rate},
};
use std::time::Instant;
use std::{
	thread::{self, sleep},
	time::Duration,
};
use sysinfo::{Disks, Networks, System};
use std::process::Command;

pub struct SystemMonitor {
	system: System,
	config: Config,
	store: SystemReportStore,
	networks: Networks,
	last_network_check: Instant,
	last_received: u64,
	last_transmitted: u64,
	gpu_monitor: GpuMonitor,
}

impl SystemMonitor {
	pub fn new(config: Config, store: SystemReportStore) -> Self {
		let mut system = System::new_all();

		system.refresh_all();

		// Initial CPU refresh with delay for accurate first reading
		system.refresh_cpu_usage();
		sleep(Duration::from_millis(200));
		system.refresh_cpu_usage();

		// Initialize networks and get initial values
		let networks = Networks::new_with_refreshed_list();
		let mut initial_received = 0u64;
		let mut initial_transmitted = 0u64;

		for (_interface_name, data) in &networks {
			initial_received += data.total_received();
			initial_transmitted += data.total_transmitted();
		}

		Self {
			system,
			config,
			store,
			networks,
			last_network_check: Instant::now(),
			last_received: initial_received,
			last_transmitted: initial_transmitted,
			gpu_monitor: GpuMonitor::new(),
		}
	}

	pub fn check_support(&self) -> bool {
		sysinfo::IS_SUPPORTED_SYSTEM
	}

	pub fn gpu_vendor(&self) -> Option<String> {
		self.gpu_monitor.detect().map(|g| g.vendor)
	}

	pub fn ram_usage(&mut self) -> u64 {
		self.system.refresh_memory();
		self.system.used_memory()
	}

	pub fn disk_usage(&mut self) -> DiskInfo {
		let disks = Disks::new_with_refreshed_list();

		// Skip ZFS disks entirely — sysinfo exposes ZFS datasets, not zpools.
		// Dataset-level values are unreliable (each dataset without a quota
		// reports the full pool size, and available/used are per-dataset).
		// ZFS capacity is obtained separately via `zpool list`.
		let mut unique_non_zfs = std::collections::HashMap::new();
		for disk in &disks {
			if disk.file_system().to_string_lossy().to_ascii_lowercase() != "zfs" {
				unique_non_zfs.insert(disk.total_space(), disk);
			}
		}

		let non_zfs_total: u64 =
			unique_non_zfs.values().map(|disk| disk.total_space()).sum();
		let non_zfs_used: u64 =
			unique_non_zfs.values().map(|disk| disk.total_space() - disk.available_space()).sum();

		let (zfs_total, zfs_used) = zpool_usage();

		DiskInfo {
			total: non_zfs_total + zfs_total,
			used: non_zfs_used + zfs_used,
		}
	}

	pub fn cpu_usage(&mut self) -> f32 {
		self.system.refresh_cpu_usage();
		self.system.global_cpu_usage()
	}

	pub fn network_usage(&mut self) -> NetworkInfo {
		self.networks.refresh(false);

		let mut total_received: u64 = 0;
		let mut total_transmitted: u64 = 0;

		for (_interface_name, data) in &self.networks {
			total_received += data.total_received();
			total_transmitted += data.total_transmitted();
		}

		// Calculate time elapsed since last check
		let now = Instant::now();
		let elapsed_seconds = now.duration_since(self.last_network_check).as_secs_f64();

		// Calculate bytes per second
		let received_per_second = if elapsed_seconds > 0.0 {
			((total_received - self.last_received) as f64 / elapsed_seconds) as u64
		} else {
			0
		};

		let transmitted_per_second = if elapsed_seconds > 0.0 {
			((total_transmitted - self.last_transmitted) as f64 / elapsed_seconds) as u64
		} else {
			0
		};

		// Update last values
		self.last_network_check = now;
		self.last_received = total_received;
		self.last_transmitted = total_transmitted;

		NetworkInfo {
			received: received_per_second,
			transmitted: transmitted_per_second,
		}
	}

	pub fn system_info(&self) -> SystemInfo {
		let total_memory = self.system.total_memory();

		SystemInfo {
			total_memory: format_bytes(total_memory, self.config.memory_unit.clone()),
		}
	}

	pub fn run(mut self) -> thread::JoinHandle<()> {
		info!("monitoring setup...");
		let info = self.system_info();

		thread::spawn(move || {
			loop {
				let ram_usage = self.ram_usage();
				let disk_info = self.disk_usage();
				let cpu_usage = self.cpu_usage();
				let network_info = self.network_usage();
				let gpu_info = self.gpu_monitor.detect();

				let (gpu_vendor, gpu_mem_used, gpu_mem_total, gpu_usage) =
					if let Some(gpu) = gpu_info {
						let used =
							gpu.used_mem.map(|m| format_bytes(m, self.config.memory_unit.clone()));
						let total =
							gpu.total_mem.map(|m| format_bytes(m, self.config.memory_unit.clone()));
						(Some(gpu.vendor), used, total, gpu.utilization)
					} else {
						(None, None, None, None)
					};

				let report = SystemReport {
					ram_total: info.total_memory.clone(),
					ram_usage: format_bytes(ram_usage, self.config.memory_unit.clone()),
					disk_total: format_bytes(disk_info.total, self.config.storage_unit.clone()),
					disk_usage: format_bytes(disk_info.used, self.config.storage_unit.clone()),
					cpu_usage: format!("{:.1}", cpu_usage),
					network_received: format_rate(
						network_info.received,
						self.config.network_unit.clone(),
					),
					network_transmitted: format_rate(
						network_info.transmitted,
						self.config.network_unit.clone(),
					),
					gpu_vendor,
					gpu_mem_used,
					gpu_mem_total,
					gpu_usage,
				};

				self.store.update(report);

				sleep(Duration::from_secs(self.config.report_interval));
			}
		})
	}
}

// Query zpools using `zpool list -Hp` which outputs raw bytes (no headers,
// tab-separated).  Column layout: NAME SIZE ALLOC FREE ...
// Returns (total_bytes, used_bytes) summed across all pools, or (0, 0) if
// zpool is not installed or fails.
fn zpool_usage() -> (u64, u64) {
	let output = Command::new("zpool").args(["list", "-Hp"]).output();
	match output {
		Ok(out) if out.status.success() => {
			let stdout = String::from_utf8_lossy(&out.stdout);
			let mut total = 0u64;
			let mut used = 0u64;
			for line in stdout.lines() {
				let parts: Vec<&str> = line.split('\t').collect();
				if parts.len() >= 3 {
					match (parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
						(Ok(size), Ok(alloc)) => {
							total += size;
							used += alloc;
						}
						_ => {
							warn!("zpool: could not parse SIZE/ALLOC for line: {}", line);
						}
					}
				}
			}
			(total, used)
		}
		Ok(out) => {
			warn!(
				"zpool list failed (exit {}): {}",
				out.status,
				String::from_utf8_lossy(&out.stderr).trim()
			);
			(0, 0)
		}
		Err(err) => {
			warn!("zpool not available: {}", err);
			(0, 0)
		}
	}
}

#[derive(Debug)]
pub struct SystemInfo {
	pub total_memory: ByteInfo,
}

#[derive(Debug)]
pub struct DiskInfo {
	pub total: u64,
	pub used: u64,
}

#[derive(Debug)]
pub struct NetworkInfo {
	pub received: u64,
	pub transmitted: u64,
}
