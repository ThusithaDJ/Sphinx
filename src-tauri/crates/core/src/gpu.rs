//! GPU capability detection (SPHIN-36): a best-effort check for whether this
//! machine has a GPU a local Ollama server can actually use. Local vision
//! models are slow enough on CPU-only hardware that the Settings screen
//! should warn about it up front rather than let the user discover it via a
//! multi-minute analysis call.
//!
//! Detection is heuristic, not exhaustive: it looks for `nvidia-smi` (the
//! common case on Windows/Linux) and falls back to an Apple Silicon check
//! (unified memory + Metal, always GPU-capable). AMD/Intel GPU users on
//! Linux (ROCm) aren't detected and will see the CPU-only warning even
//! though Ollama may in fact use their GPU -- acceptable for a warning
//! that's advisory, not a hard gate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub available: bool,
    /// Short machine-readable backend id: "nvidia", "apple-metal", "none".
    pub backend: String,
    /// Human-readable detail (GPU name, or an explanation when unavailable).
    pub detail: String,
}

/// Probe this machine for a usable GPU. Never fails -- an inconclusive probe
/// just reports `available: false` with an explanatory `detail`, since this
/// is advisory information for the Settings UI, not a hard requirement.
pub fn detect_gpu() -> GpuInfo {
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if let Some(name) = parse_nvidia_smi_output(
            output.status.success(),
            &String::from_utf8_lossy(&output.stdout),
        ) {
            return GpuInfo {
                available: true,
                backend: "nvidia".to_string(),
                detail: name,
            };
        }
    }

    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        return GpuInfo {
            available: true,
            backend: "apple-metal".to_string(),
            detail: "Apple Silicon (Metal)".to_string(),
        };
    }

    GpuInfo {
        available: false,
        backend: "none".to_string(),
        detail: "no GPU detected — local models will run on CPU and may be slow".to_string(),
    }
}

/// Pure parse of `nvidia-smi --query-gpu=name --format=csv,noheader` output,
/// split out so the decision logic is unit-testable without actually
/// shelling out.
fn parse_nvidia_smi_output(success: bool, stdout: &str) -> Option<String> {
    if !success {
        return None;
    }
    let name = stdout.lines().next().unwrap_or("").trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_gpu_name() {
        assert_eq!(
            parse_nvidia_smi_output(true, "NVIDIA GeForce RTX 4090\n"),
            Some("NVIDIA GeForce RTX 4090".to_string())
        );
    }

    #[test]
    fn takes_the_first_gpu_when_multiple_are_listed() {
        assert_eq!(
            parse_nvidia_smi_output(true, "NVIDIA RTX A6000\nNVIDIA RTX A6000\n"),
            Some("NVIDIA RTX A6000".to_string())
        );
    }

    #[test]
    fn a_failed_command_yields_nothing() {
        assert_eq!(parse_nvidia_smi_output(false, "NVIDIA RTX 4090\n"), None);
    }

    #[test]
    fn blank_output_yields_nothing() {
        assert_eq!(parse_nvidia_smi_output(true, "\n  \n"), None);
    }
}
