use nvml_wrapper::Nvml;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct GpuInfo {
	pub vendor: String,
	pub used_mem: Option<u64>,
	pub total_mem: Option<u64>,
	pub utilization: Option<f64>,
}

pub struct GpuMonitor {
	nvml: Option<Nvml>,
}

impl GpuMonitor {
	pub fn new() -> Self {
		let nvml = Nvml::init().ok();
		Self {
			nvml,
		}
	}

	pub fn detect(&self) -> Option<GpuInfo> {
		if let Some(info) = self.detect_nvidia() {
			return Some(info);
		}
		if let Some(info) = Self::detect_amd() {
			return Some(info);
		}
		Self::detect_intel()
	}

	fn detect_nvidia(&self) -> Option<GpuInfo> {
		let nvml = self.nvml.as_ref()?;
		let device = nvml.device_by_index(0).ok()?;
		let mem = device.memory_info().ok()?;
		let util = device.utilization_rates().ok();

		Some(GpuInfo {
			vendor: "NVIDIA".to_string(),
			used_mem: Some(mem.used),
			total_mem: Some(mem.total),
			utilization: util.map(|u| u.gpu as f64),
		})
	}

	fn detect_amd() -> Option<GpuInfo> {
		let drm_path = Path::new("/sys/class/drm");
		let dir = fs::read_dir(drm_path).ok()?;

		for entry in dir {
			let entry = match entry {
				Ok(e) => e,
				Err(_) => continue,
			};
			let path = entry.path();
			let vendor = match fs::read_to_string(path.join("device/vendor")) {
				Ok(v) => v,
				Err(_) => continue,
			};
			if vendor.trim() != "0x1002" {
				continue;
			}

			let used_mem = fs::read_to_string(path.join("device/mem_info_vram_used"))
				.ok()
				.and_then(|s| s.trim().parse::<u64>().ok());
			let total_mem = fs::read_to_string(path.join("device/mem_info_vram_total"))
				.ok()
				.and_then(|s| s.trim().parse::<u64>().ok());
			let utilization = fs::read_to_string(path.join("device/gpu_busy_percent"))
				.ok()
				.and_then(|s| s.trim().parse::<f64>().ok());

			return Some(GpuInfo {
				vendor: "AMD".to_string(),
				used_mem,
				total_mem,
				utilization,
			});
		}
		None
	}

	fn detect_intel() -> Option<GpuInfo> {
		let drm_path = Path::new("/sys/class/drm");
		let dir = fs::read_dir(drm_path).ok()?;

		for entry in dir {
			let entry = match entry {
				Ok(e) => e,
				Err(_) => continue,
			};
			let path = entry.path();
			let vendor = match fs::read_to_string(path.join("device/vendor")) {
				Ok(v) => v,
				Err(_) => continue,
			};
			if vendor.trim() != "0x8086" {
				continue;
			}

			// Try VRAM stats — available on Intel Arc (xe driver, kernel 6.10+)
			let used_mem = fs::read_to_string(path.join("device/mem_info_vram_used"))
				.ok()
				.and_then(|s| s.trim().parse::<u64>().ok());
			let total_mem = fs::read_to_string(path.join("device/mem_info_vram_total"))
				.ok()
				.and_then(|s| s.trim().parse::<u64>().ok());

			// Try utilization via frequency ratio.
			// i915 driver path:  gt/gt0/rps_act_freq_mhz + rps_max_freq_mhz
			// xe driver path:    device/tile0/gt0/freq0/act_freq + max_freq
			let utilization = Self::intel_utilization_from_freq(&path);

			return Some(GpuInfo {
				vendor: "Intel".to_string(),
				used_mem,
				total_mem,
				utilization,
			});
		}
		None
	}

	fn intel_utilization_from_freq(card_path: &Path) -> Option<f64> {
		// i915 driver (iGPU and older discrete: DG1/DG2)
		let i915_act = card_path.join("gt/gt0/rps_act_freq_mhz");
		let i915_max = card_path.join("gt/gt0/rps_max_freq_mhz");
		if let (Some(act), Some(max)) = (
			fs::read_to_string(&i915_act).ok().and_then(|s| s.trim().parse::<f64>().ok()),
			fs::read_to_string(&i915_max).ok().and_then(|s| s.trim().parse::<f64>().ok()),
		) {
			if max > 0.0 {
				return Some((act / max * 100.0).min(100.0));
			}
		}

		// xe driver (Intel Arc / Meteor Lake onward, kernel 6.8+)
		let xe_act = card_path.join("device/tile0/gt0/freq0/act_freq");
		let xe_max = card_path.join("device/tile0/gt0/freq0/max_freq");
		if let (Some(act), Some(max)) = (
			fs::read_to_string(&xe_act).ok().and_then(|s| s.trim().parse::<f64>().ok()),
			fs::read_to_string(&xe_max).ok().and_then(|s| s.trim().parse::<f64>().ok()),
		) {
			if max > 0.0 {
				return Some((act / max * 100.0).min(100.0));
			}
		}

		None
	}
}
