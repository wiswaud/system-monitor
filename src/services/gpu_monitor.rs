use nvml_wrapper::Nvml;
use std::fs;
use std::path::Path;

// PCI vendor IDs for virtual/hypervisor display adapters that must never be
// reported as real GPUs.
const VIRTUAL_VENDOR_IDS: &[&str] = &[
	"0x1af4", // VirtIO / Red Hat (QEMU virtio-gpu)
	"0x1234", // QEMU standard VGA
	"0x15ad", // VMware SVGA
	"0x80ee", // VirtualBox Graphics Adapter
];

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

	/// Returns `true` for virtual/hypervisor display adapter vendor IDs that
	/// must never be treated as real GPUs.
	fn is_virtual_vendor(vendor: &str) -> bool {
		VIRTUAL_VENDOR_IDS.contains(&vendor)
	}

	/// Iterates `/sys/class/drm` and yields only the paths of `cardN` entries
	/// (skipping render nodes such as `renderD128` and other non-card entries).
	fn drm_card_paths() -> Vec<std::path::PathBuf> {
		let Ok(dir) = fs::read_dir("/sys/class/drm") else {
			return Vec::new();
		};
		dir.filter_map(|e| e.ok())
			.filter(|e| {
				e.file_name()
					.to_str()
					.map(|n| {
						n.starts_with("card")
							&& n.chars().nth(4).map_or(false, |c| c.is_ascii_digit())
					})
					.unwrap_or(false)
			})
			.map(|e| e.path())
			.collect()
	}

	fn detect_amd() -> Option<GpuInfo> {
		for path in Self::drm_card_paths() {
			let vendor = match fs::read_to_string(path.join("device/vendor")) {
				Ok(v) => v,
				Err(_) => continue,
			};
			let vendor = vendor.trim();
			if Self::is_virtual_vendor(vendor) || vendor != "0x1002" {
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
		for path in Self::drm_card_paths() {
			let vendor = match fs::read_to_string(path.join("device/vendor")) {
				Ok(v) => v,
				Err(_) => continue,
			};
			let vendor = vendor.trim();
			if Self::is_virtual_vendor(vendor) || vendor != "0x8086" {
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
