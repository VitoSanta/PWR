//! Cross-platform hardware profiling and memory-pressure observation.
//!
//! Every fact here is observed or absent. A probe that cannot run does not
//! yield a default: it names itself in `unavailable_fields`, because a machine
//! whose memory could not be read and a machine with no memory free are the
//! same value and opposite situations, and admission has to be able to tell
//! them apart.
//!
//! The macOS path deliberately keeps the exact fields and spellings the
//! previous macOS-only probe produced -- `uname -s` reports `Darwin`, not
//! `macos` -- because `compatibility_key` is hashed from them and every
//! calibration already on disk is matched by that key. A tidier spelling would
//! silently invalidate every measurement this project has taken.

use pwr_domain::{HardwareProfile, Observation, Provenance, hash_bytes, new_id, now};
use std::time::Duration;

/// How long any single probe command may take before it is treated as absent.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Reads the host's hardware facts using whatever this platform provides.
pub async fn probe_hardware() -> HardwareProfile {
    let platform = Platform::current();
    let facts = match platform {
        Platform::MacOs => macos_facts().await,
        Platform::Linux => linux_facts().await,
        Platform::Windows => windows_facts().await,
        Platform::Unknown => HardwareFacts::default(),
    };
    let os = facts
        .os
        .unwrap_or_else(|| canonical_os(std::env::consts::OS).to_owned());
    let architecture = facts
        .architecture
        .unwrap_or_else(|| std::env::consts::ARCH.to_owned());
    let cpu = facts.cpu.unwrap_or_else(|| "unknown".into());
    let mut unavailable = Vec::new();
    if facts.total_memory_bytes.is_none() {
        unavailable.push("total_memory_bytes".into());
    }
    if facts.storage_free_bytes.is_none() {
        unavailable.push("storage_free_bytes".into());
    }
    // An empty accelerator list is not the same claim as "this machine has no
    // accelerator": on a host whose probe could not run we know nothing, and
    // saying nothing is the honest form of that.
    if !facts.accelerators_observed {
        unavailable.push("accelerators".into());
    }
    if platform == Platform::Unknown {
        unavailable.push("platform_probe".into());
    }
    HardwareProfile {
        schema_version: 1,
        id: new_id(),
        // Unchanged from the macOS-only probe, so a calibration measured
        // before this profiler existed still matches the machine it was taken
        // on.
        compatibility_key: hash_bytes(format!(
            "{}|{}|{}|{:?}",
            os, architecture, cpu, facts.total_memory_bytes
        )),
        os,
        architecture,
        cpu,
        accelerators: facts.accelerators,
        total_memory_bytes: facts.total_memory_bytes,
        storage_free_bytes: facts.storage_free_bytes,
        unavailable_fields: unavailable,
        probe_version: platform.probe_version().into(),
        provenance: Provenance {
            source: platform.probe_source().into(),
            observed_at: now(),
            content_hash: "probe-output-not-persisted".into(),
        },
    }
}

/// Memory pressure on this host, or `Unknown`.
///
/// `free_percent_floor` is a declared policy floor, not an inferred capacity:
/// it decides when an observed free-memory reading counts as pressure, and it
/// is recorded alongside the reading so the judgement can be re-examined.
pub struct HostMemoryProbe {
    pub free_percent_floor: u8,
}

impl HostMemoryProbe {
    pub async fn observe(&self) -> Observation {
        let Observation::Observed(value) = read_pressure().await else {
            return read_pressure().await;
        };
        let Some(free) = value
            .get("system_free_percent")
            .and_then(serde_json::Value::as_u64)
        else {
            // A reading we cannot interpret is unknown, never "no pressure".
            return Observation::Unknown {
                reason: "memory pressure reading had no free percentage".into(),
            };
        };
        let source = value
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        Observation::Observed(serde_json::json!({
            "system_free_percent": free,
            "free_percent_floor": self.free_percent_floor,
            "under_pressure": free < u64::from(self.free_percent_floor),
            "source": source,
        }))
    }
}

async fn read_pressure() -> Observation {
    match Platform::current() {
        Platform::MacOs => macos_pressure().await,
        Platform::Linux => linux_pressure().await,
        Platform::Windows => windows_pressure().await,
        Platform::Unknown => Observation::Unknown {
            reason: "no memory pressure probe exists for this platform".into(),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    MacOs,
    Linux,
    Windows,
    Unknown,
}

impl Platform {
    fn current() -> Self {
        match std::env::consts::OS {
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            "windows" => Self::Windows,
            _ => Self::Unknown,
        }
    }

    fn probe_version(self) -> &'static str {
        match self {
            // Unchanged: the string is recorded in artifacts already written.
            Self::MacOs => "macos-command-probe-v1",
            Self::Linux => "linux-procfs-probe-v1",
            Self::Windows => "windows-cim-probe-v1",
            Self::Unknown => "unsupported-platform-probe-v1",
        }
    }

    fn probe_source(self) -> &'static str {
        match self {
            Self::MacOs => "uname,sysctl,df (no identifiers)",
            Self::Linux => "uname,/proc/cpuinfo,/proc/meminfo,df,nvidia-smi (no identifiers)",
            Self::Windows => "powershell Get-CimInstance (no identifiers)",
            Self::Unknown => "no probe available for this platform",
        }
    }
}

/// Facts a platform probe managed to read. Every field is optional because
/// every one of them can be unreadable.
#[derive(Default)]
struct HardwareFacts {
    os: Option<String>,
    architecture: Option<String>,
    cpu: Option<String>,
    total_memory_bytes: Option<u64>,
    storage_free_bytes: Option<u64>,
    accelerators: Vec<String>,
    /// Whether the accelerator probe ran at all, as distinct from running and
    /// finding none.
    accelerators_observed: bool,
}

/// `uname -s` spellings, so a host with no `uname` lands on the same string
/// the platform's own probe would have produced.
fn canonical_os(os: &str) -> &str {
    match os {
        "macos" => "Darwin",
        "linux" => "Linux",
        "windows" => "Windows_NT",
        other => other,
    }
}

async fn macos_facts() -> HardwareFacts {
    let cpu = run_probe("sysctl", &["-n", "machdep.cpu.brand_string"]).await;
    // Apple Silicon shares one memory pool between CPU and GPU, so the
    // accelerator's capacity is the machine's memory rather than a separate
    // figure to be discovered.
    let accelerators = match cpu.as_deref() {
        Some(brand) if brand.starts_with("Apple ") => vec![format!("{brand} (unified memory)")],
        _ => Vec::new(),
    };
    HardwareFacts {
        os: run_probe("uname", &["-s"]).await,
        architecture: run_probe("uname", &["-m"]).await,
        cpu,
        total_memory_bytes: run_probe("sysctl", &["-n", "hw.memsize"])
            .await
            .and_then(|value| value.parse().ok()),
        storage_free_bytes: free_storage_from_df().await,
        accelerators,
        accelerators_observed: true,
    }
}

async fn linux_facts() -> HardwareFacts {
    let cpu = tokio::fs::read_to_string("/proc/cpuinfo")
        .await
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.trim() == "model name")
                })
                .map(|(_, value)| value.trim().to_owned())
        });
    let total_memory_bytes = tokio::fs::read_to_string("/proc/meminfo")
        .await
        .ok()
        .and_then(|text| meminfo_kib(&text, "MemTotal"))
        .map(|kib| kib * 1024);
    // Absent tooling is absent evidence: a machine with no `nvidia-smi` may
    // still have a GPU this build cannot see, so the list stays empty and the
    // field is still reported as observed only when the probe ran.
    let (accelerators, accelerators_observed) = match run_probe(
        "nvidia-smi",
        &["--query-gpu=name,memory.total", "--format=csv,noheader"],
    )
    .await
    {
        Some(output) => (
            output
                .lines()
                .map(|line| line.trim().to_owned())
                .filter(|line| !line.is_empty())
                .collect(),
            true,
        ),
        None => (Vec::new(), false),
    };
    HardwareFacts {
        os: run_probe("uname", &["-s"]).await,
        architecture: run_probe("uname", &["-m"]).await,
        cpu,
        total_memory_bytes,
        storage_free_bytes: free_storage_from_df().await,
        accelerators,
        accelerators_observed,
    }
}

async fn windows_facts() -> HardwareFacts {
    // One PowerShell invocation rather than four: process start-up dominates
    // the cost of these reads on Windows.
    let facts = run_probe(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$os = Get-CimInstance Win32_OperatingSystem; \
             $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1; \
             $gpu = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }); \
             [pscustomobject]@{ \
               cpu = $cpu.Name; \
               total_memory_bytes = $os.TotalVisibleMemorySize * 1024; \
               free_memory_bytes = $os.FreePhysicalMemory * 1024; \
               accelerators = $gpu \
             } | ConvertTo-Json -Compress",
        ],
    )
    .await
    .and_then(|output| serde_json::from_str::<serde_json::Value>(&output).ok());
    let text = |field: &str| {
        facts
            .as_ref()
            .and_then(|value| value.get(field))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let accelerators: Vec<String> = facts
        .as_ref()
        .and_then(|value| value.get("accelerators"))
        .map(|value| match value {
            // A single controller is serialized as a bare string.
            serde_json::Value::String(name) => vec![name.clone()],
            serde_json::Value::Array(names) => names
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        })
        .unwrap_or_default();
    HardwareFacts {
        os: Some(canonical_os("windows").to_owned()),
        architecture: Some(std::env::consts::ARCH.to_owned()),
        cpu: text("cpu"),
        total_memory_bytes: facts
            .as_ref()
            .and_then(|value| value.get("total_memory_bytes"))
            .and_then(serde_json::Value::as_u64),
        storage_free_bytes: free_storage_from_powershell().await,
        accelerators,
        accelerators_observed: facts.is_some(),
    }
}

async fn macos_pressure() -> Observation {
    let Some(output) = run_probe("memory_pressure", &["-Q"]).await else {
        return Observation::Unknown {
            reason: "memory_pressure command unavailable".into(),
        };
    };
    let prefix = "System-wide memory free percentage:";
    match output
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .and_then(|value| value.trim().trim_end_matches('%').parse::<u8>().ok())
    {
        Some(percent) => Observation::Observed(
            serde_json::json!({"system_free_percent": percent, "source": "memory_pressure -Q"}),
        ),
        None => Observation::Unknown {
            reason: "memory_pressure output did not contain a parseable free percentage".into(),
        },
    }
}

async fn linux_pressure() -> Observation {
    let Ok(text) = tokio::fs::read_to_string("/proc/meminfo").await else {
        return Observation::Unknown {
            reason: "/proc/meminfo is unreadable".into(),
        };
    };
    // `MemAvailable` rather than `MemFree`: reclaimable cache is memory a new
    // model can have, and treating it as used reports pressure that is not
    // there.
    let (Some(total), Some(available)) = (
        meminfo_kib(&text, "MemTotal"),
        meminfo_kib(&text, "MemAvailable"),
    ) else {
        return Observation::Unknown {
            reason: "/proc/meminfo did not report MemTotal and MemAvailable".into(),
        };
    };
    if total == 0 {
        return Observation::Unknown {
            reason: "/proc/meminfo reported zero total memory".into(),
        };
    }
    Observation::Observed(serde_json::json!({
        "system_free_percent": available.saturating_mul(100) / total,
        "source": "/proc/meminfo MemAvailable",
    }))
}

async fn windows_pressure() -> Observation {
    let Some(output) = run_probe(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$os = Get-CimInstance Win32_OperatingSystem; \
             [int](100 * $os.FreePhysicalMemory / $os.TotalVisibleMemorySize)",
        ],
    )
    .await
    else {
        return Observation::Unknown {
            reason: "Get-CimInstance Win32_OperatingSystem was unavailable".into(),
        };
    };
    match output.trim().parse::<u8>() {
        Ok(percent) => Observation::Observed(serde_json::json!({
            "system_free_percent": percent,
            "source": "Win32_OperatingSystem FreePhysicalMemory",
        })),
        Err(_) => Observation::Unknown {
            reason: "Win32_OperatingSystem did not report a parseable free percentage".into(),
        },
    }
}

fn meminfo_kib(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.split_once(':').filter(|(field, _)| *field == key))
        .and_then(|(_, value)| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
}

async fn free_storage_from_df() -> Option<u64> {
    let output = run_probe("df", &["-k", "."]).await?;
    output
        .lines()
        .nth(1)
        .and_then(|line| line.split_whitespace().nth(3))
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib * 1024)
}

async fn free_storage_from_powershell() -> Option<u64> {
    let output = run_probe(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-PSDrive -Name (Get-Location).Drive.Name).Free",
        ],
    )
    .await?;
    output.trim().parse().ok()
}

async fn run_probe(command: &str, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        tokio::process::Command::new(command).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_is_read_by_field_and_never_by_position() {
        let text = "MemTotal:       16316108 kB\nMemFree:          204832 kB\nMemAvailable:    8123404 kB\n";
        assert_eq!(meminfo_kib(text, "MemTotal"), Some(16_316_108));
        assert_eq!(meminfo_kib(text, "MemAvailable"), Some(8_123_404));
        // A field this kernel does not publish is absent, not zero.
        assert_eq!(meminfo_kib(text, "MemShmem"), None);
    }

    #[test]
    fn a_host_with_no_uname_still_lands_on_the_platform_spelling() {
        // The compatibility key is hashed from these strings, and a calibration
        // is matched by that key, so the fallback has to agree with what the
        // command would have said.
        assert_eq!(canonical_os("macos"), "Darwin");
        assert_eq!(canonical_os("linux"), "Linux");
        assert_eq!(canonical_os("windows"), "Windows_NT");
    }

    #[tokio::test]
    async fn a_probe_that_cannot_run_reports_absence_rather_than_a_default() {
        assert_eq!(
            run_probe("pwr-no-such-command-exists", &[]).await,
            None,
            "a missing command must not be reported as empty output"
        );
    }

    #[tokio::test]
    async fn the_profile_names_every_fact_it_could_not_read() {
        let profile = probe_hardware().await;
        // Whatever this host is, a fact is either present or named as missing.
        // The pair being inconsistent is the failure that lets admission read
        // an unreadable machine as an empty one.
        assert_eq!(
            profile.total_memory_bytes.is_none(),
            profile
                .unavailable_fields
                .contains(&"total_memory_bytes".to_string())
        );
        assert!(!profile.probe_version.is_empty());
        assert!(!profile.compatibility_key.is_empty());
    }

    #[tokio::test]
    async fn observed_pressure_carries_the_floor_it_was_judged_against() {
        let observation = HostMemoryProbe {
            free_percent_floor: 20,
        }
        .observe()
        .await;
        // On a host with no pressure source this is `Unknown`, which is the
        // point: it is never reported as "no pressure".
        if let Observation::Observed(value) = observation {
            assert_eq!(value["free_percent_floor"], 20);
            assert!(value["under_pressure"].is_boolean());
            assert!(value["source"].is_string());
        }
    }
}
