//! A normalized description of this machine, for model recommendations.
//!
//! [`crate::hardware::probe_hardware`] produces the profile calibration is
//! keyed on, and its fields and spellings are frozen by that key. This is the
//! other consumer's view: what the app and the model manager need to judge
//! whether a model can be loaded here -- memory (and whether it is unified),
//! the GPU and its memory where that can be read reliably, the OS version and
//! the free space where models are stored.
//!
//! Facts come from the `sysinfo` library (memory, OS, CPU, disks) rather than
//! from parsing command output, and GPU memory only from sources that report
//! it exactly: Apple Silicon's memory is the machine's memory, and an NVIDIA
//! card's comes from `nvidia-smi`. Windows' `Win32_VideoController.AdapterRAM`
//! is a 32-bit field that wraps above 4 GiB, so it is never used for a size.
//! Anything that could not be read is `None` and named in `unknown`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostPlatform {
    Macos,
    Windows,
    Linux,
    Unknown,
}

impl HostPlatform {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Apple,
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: GpuVendor,
    /// Dedicated memory, only where a source reports it exactly.
    pub vram_bytes: Option<u64>,
    /// Where `vram_bytes` came from, or why it is absent.
    pub vram_source: String,
    /// The GPU shares the machine's memory (Apple Silicon).
    pub unified_memory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleChip {
    /// As the system names it: "Apple M3 Max".
    pub name: String,
    /// The M-series generation, 1 for M1.
    pub generation: Option<u8>,
    /// "Pro", "Max", "Ultra", or `None` for the base chip.
    pub tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFacts {
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    /// macOS compresses and reclaims memory on demand, so what is "available"
    /// there is the OS's estimate, not a reservation.
    pub available_is_estimate: bool,
    /// CPU and GPU share this memory.
    pub unified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskFacts {
    /// The directory the figure is for: where models are stored.
    pub path: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostProfile {
    pub schema_version: u32,
    pub platform: HostPlatform,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    /// `arm64` or `x86_64`, whatever the platform calls it.
    pub architecture: String,
    pub cpu: Option<String>,
    pub physical_cores: Option<usize>,
    pub logical_cores: Option<usize>,
    pub apple_chip: Option<AppleChip>,
    pub memory: MemoryFacts,
    pub gpus: Vec<GpuInfo>,
    pub disk: Option<DiskFacts>,
    /// Facts that could not be read, by name.
    pub unknown: Vec<String>,
}

impl HostProfile {
    pub fn is_apple_silicon(&self) -> bool {
        self.platform == HostPlatform::Macos && self.architecture == "arm64"
    }

    /// Dedicated GPU memory of the largest GPU that reports it exactly.
    pub fn largest_vram_bytes(&self) -> Option<u64> {
        self.gpus.iter().filter_map(|gpu| gpu.vram_bytes).max()
    }

    /// The artifact platform, as the declared registry names it.
    pub fn artifact_platform(&self) -> Option<pwr_domain::ArtifactPlatform> {
        use pwr_domain::ArtifactPlatform;
        match (self.platform, self.architecture.as_str()) {
            (HostPlatform::Macos, "arm64") => Some(ArtifactPlatform::MacosAppleSilicon),
            (HostPlatform::Windows, "x86_64") => Some(ArtifactPlatform::WindowsX86_64),
            (HostPlatform::Linux, "x86_64") => Some(ArtifactPlatform::LinuxX86_64),
            _ => None,
        }
    }
}

/// Reads this machine. `models_root` is where models are stored; its free
/// space is the figure that matters for a download.
pub async fn detect_host(models_root: &Path) -> HostProfile {
    let models_root = models_root.to_path_buf();
    let base = tokio::task::spawn_blocking(move || system_facts(&models_root))
        .await
        .unwrap_or_else(|_| empty_profile());
    let mut profile = base;
    let nvidia = run_probe(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )
    .await
    .map(|output| parse_nvidia_smi(&output))
    .unwrap_or_default();
    match profile.platform {
        HostPlatform::Macos => {
            if let Some(chip) = &profile.apple_chip {
                profile.gpus.push(GpuInfo {
                    name: format!("{} GPU", chip.name),
                    vendor: GpuVendor::Apple,
                    vram_bytes: None,
                    vram_source: "unified memory: shares the machine's memory".into(),
                    unified_memory: true,
                });
            }
        }
        HostPlatform::Windows => {
            let names = windows_gpu_names().await;
            profile.gpus = merge_gpus(names, nvidia);
        }
        HostPlatform::Linux | HostPlatform::Unknown => profile.gpus = nvidia,
    }
    if profile.gpus.is_empty() && profile.platform != HostPlatform::Macos {
        profile.unknown.push("gpus".into());
    }
    profile
}

fn empty_profile() -> HostProfile {
    HostProfile {
        schema_version: 1,
        platform: HostPlatform::current(),
        os_name: None,
        os_version: None,
        architecture: normalize_arch(std::env::consts::ARCH),
        cpu: None,
        physical_cores: None,
        logical_cores: None,
        apple_chip: None,
        memory: MemoryFacts {
            total_bytes: None,
            available_bytes: None,
            available_is_estimate: true,
            unified: false,
        },
        gpus: Vec::new(),
        disk: None,
        unknown: vec!["system".into()],
    }
}

fn system_facts(models_root: &Path) -> HostProfile {
    use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};
    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::nothing()),
    );
    let platform = HostPlatform::current();
    let architecture = normalize_arch(&System::cpu_arch());
    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .filter(|brand| !brand.is_empty());
    let apple_chip = (platform == HostPlatform::Macos)
        .then(|| cpu.as_deref().and_then(apple_chip))
        .flatten();
    let nonzero = |value: u64| (value > 0).then_some(value);
    let memory = MemoryFacts {
        total_bytes: nonzero(system.total_memory()),
        available_bytes: nonzero(system.available_memory()),
        available_is_estimate: platform == HostPlatform::Macos,
        unified: apple_chip.is_some(),
    };
    let disks = Disks::new_with_refreshed_list();
    let disk = disk_for(
        &nearest_existing(models_root),
        disks.list().iter().map(|disk| {
            (
                disk.mount_point().to_path_buf(),
                disk.available_space(),
                disk.total_space(),
            )
        }),
    )
    .map(|(free_bytes, total_bytes)| DiskFacts {
        path: models_root.display().to_string(),
        free_bytes,
        total_bytes,
    });
    let logical = system.cpus().len();
    let mut profile = HostProfile {
        schema_version: 1,
        platform,
        os_name: System::name(),
        os_version: System::os_version(),
        architecture,
        cpu,
        physical_cores: System::physical_core_count(),
        logical_cores: (logical > 0).then_some(logical),
        apple_chip,
        memory,
        gpus: Vec::new(),
        disk,
        unknown: Vec::new(),
    };
    let mut unknown = Vec::new();
    for (name, missing) in [
        ("osVersion", profile.os_version.is_none()),
        ("cpu", profile.cpu.is_none()),
        ("memory.totalBytes", profile.memory.total_bytes.is_none()),
        (
            "memory.availableBytes",
            profile.memory.available_bytes.is_none(),
        ),
        ("disk", profile.disk.is_none()),
    ] {
        if missing {
            unknown.push(name.to_owned());
        }
    }
    profile.unknown = unknown;
    profile
}

/// Free and total bytes on the disk holding `path` (or its nearest existing
/// ancestor), from the OS's own accounting; `None` if no disk contains it.
pub fn free_space(path: &Path) -> Option<(u64, u64)> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disk_for(
        &nearest_existing(path),
        disks.list().iter().map(|disk| {
            (
                disk.mount_point().to_path_buf(),
                disk.available_space(),
                disk.total_space(),
            )
        }),
    )
}

/// The disk holding `path`: the mount point that is its longest prefix.
fn disk_for(path: &Path, disks: impl Iterator<Item = (PathBuf, u64, u64)>) -> Option<(u64, u64)> {
    disks
        .filter(|(mount, _, _)| path.starts_with(mount))
        .max_by_key(|(mount, _, _)| mount.components().count())
        .map(|(_, free, total)| (free, total))
}

fn nearest_existing(path: &Path) -> PathBuf {
    let mut candidate = path;
    loop {
        if candidate.exists() {
            return candidate
                .canonicalize()
                .unwrap_or_else(|_| candidate.to_path_buf());
        }
        match candidate.parent() {
            Some(parent) => candidate = parent,
            None => return PathBuf::from("/"),
        }
    }
}

/// `aarch64` and `arm64` are one architecture; so are `amd64` and `x86_64`.
pub fn normalize_arch(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "aarch64" | "arm64" => "arm64".into(),
        "x86_64" | "amd64" | "x64" => "x86_64".into(),
        other => other.into(),
    }
}

/// An Apple Silicon chip from the CPU brand string, `None` for anything else.
pub fn apple_chip(brand: &str) -> Option<AppleChip> {
    let name = brand.trim();
    let rest = name.strip_prefix("Apple M")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let generation = digits.parse::<u8>().ok();
    let tier = rest[digits.len()..]
        .split_whitespace()
        .next()
        .filter(|tier| matches!(*tier, "Pro" | "Max" | "Ultra"))
        .map(str::to_owned);
    Some(AppleChip {
        name: name.to_owned(),
        generation,
        tier,
    })
}

pub fn gpu_vendor(name: &str) -> GpuVendor {
    let lower = name.to_ascii_lowercase();
    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") {
        GpuVendor::Nvidia
    } else if lower.contains("amd") || lower.contains("radeon") {
        GpuVendor::Amd
    } else if lower.contains("intel") {
        GpuVendor::Intel
    } else if lower.starts_with("apple") {
        GpuVendor::Apple
    } else {
        GpuVendor::Other
    }
}

/// `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`:
/// one line per GPU, memory in MiB.
pub fn parse_nvidia_smi(output: &str) -> Vec<GpuInfo> {
    output
        .lines()
        .filter_map(|line| {
            let (name, mib) = line.rsplit_once(',')?;
            let mib = mib.trim().parse::<u64>().ok()?;
            let name = name.trim();
            (!name.is_empty()).then(|| GpuInfo {
                name: name.to_owned(),
                vendor: GpuVendor::Nvidia,
                vram_bytes: Some(mib * 1024 * 1024),
                vram_source: "nvidia-smi".into(),
                unified_memory: false,
            })
        })
        .collect()
}

/// Windows' controller names, with NVIDIA memory from `nvidia-smi` joined on
/// by name; every other controller's memory stays unknown.
fn merge_gpus(names: Vec<String>, nvidia: Vec<GpuInfo>) -> Vec<GpuInfo> {
    let mut gpus: Vec<GpuInfo> = names
        .into_iter()
        .filter(|name| !name.to_ascii_lowercase().contains("basic display"))
        .map(|name| {
            nvidia
                .iter()
                .find(|gpu| gpu.name.eq_ignore_ascii_case(&name))
                .cloned()
                .unwrap_or_else(|| GpuInfo {
                    vendor: gpu_vendor(&name),
                    name,
                    vram_bytes: None,
                    vram_source: "not reported reliably for this GPU".into(),
                    unified_memory: false,
                })
        })
        .collect();
    for gpu in nvidia {
        if !gpus
            .iter()
            .any(|known| known.name.eq_ignore_ascii_case(&gpu.name))
        {
            gpus.push(gpu);
        }
    }
    gpus
}

async fn windows_gpu_names() -> Vec<String> {
    run_probe(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }",
        ],
    )
    .await
    .map(|output| {
        output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

async fn run_probe(command: &str, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        tokio::process::Command::new(command).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architectures_are_named_one_way() {
        assert_eq!(normalize_arch("aarch64"), "arm64");
        assert_eq!(normalize_arch("arm64"), "arm64");
        assert_eq!(normalize_arch("AMD64"), "x86_64");
        assert_eq!(normalize_arch("x86_64"), "x86_64");
        assert_eq!(normalize_arch("riscv64"), "riscv64");
    }

    #[test]
    fn apple_chips_are_read_from_the_brand_string() {
        let chip = apple_chip("Apple M3 Max").unwrap();
        assert_eq!(chip.generation, Some(3));
        assert_eq!(chip.tier.as_deref(), Some("Max"));
        let base = apple_chip("Apple M1").unwrap();
        assert_eq!(base.generation, Some(1));
        assert_eq!(base.tier, None);
        assert_eq!(
            apple_chip("Apple M2 Ultra").unwrap().tier.as_deref(),
            Some("Ultra")
        );
        assert!(apple_chip("Intel(R) Core(TM) i9-9880H CPU @ 2.30GHz").is_none());
    }

    #[test]
    fn nvidia_smi_memory_is_read_in_mebibytes() {
        let gpus = parse_nvidia_smi(
            "NVIDIA GeForce RTX 4070, 12282\nNVIDIA RTX A2000, 6138\nnot a line\n",
        );
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 4070");
        assert_eq!(gpus[0].vram_bytes, Some(12_282 * 1024 * 1024));
        assert_eq!(gpus[1].vram_bytes, Some(6_138 * 1024 * 1024));
    }

    #[test]
    fn windows_controllers_keep_unknown_memory_unknown() {
        let gpus = merge_gpus(
            vec![
                "NVIDIA GeForce RTX 3060".into(),
                "Intel(R) UHD Graphics 770".into(),
                "Microsoft Basic Display Adapter".into(),
            ],
            parse_nvidia_smi("NVIDIA GeForce RTX 3060, 12288"),
        );
        assert_eq!(gpus.len(), 2, "{gpus:?}");
        assert_eq!(gpus[0].vram_bytes, Some(12_288 * 1024 * 1024));
        assert_eq!(gpus[1].vendor, GpuVendor::Intel);
        assert_eq!(gpus[1].vram_bytes, None, "an unreliable size was reported");
    }

    #[test]
    fn free_space_is_read_from_the_disk_holding_the_path() {
        let disks = vec![
            (PathBuf::from("/"), 10, 100),
            (PathBuf::from("/Volumes/Models"), 500, 1000),
        ];
        assert_eq!(
            disk_for(
                Path::new("/Volumes/Models/lmstudio"),
                disks.clone().into_iter()
            ),
            Some((500, 1000))
        );
        assert_eq!(
            disk_for(Path::new("/Users/me/.lmstudio"), disks.into_iter()),
            Some((10, 100))
        );
    }

    #[tokio::test]
    async fn the_profile_names_what_it_could_not_read() {
        let profile = detect_host(Path::new(".")).await;
        assert_eq!(profile.schema_version, 1);
        assert_eq!(
            profile.memory.total_bytes.is_none(),
            profile.unknown.contains(&"memory.totalBytes".to_string())
        );
        if profile.is_apple_silicon() {
            assert!(profile.memory.unified);
            assert!(profile.apple_chip.is_some());
        }
    }
}
