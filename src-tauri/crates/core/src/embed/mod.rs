//! SPHIN-4: write generated metadata into the asset file itself as IPTC/XMP
//! (SPHIN-20), and export it to CSV for manual review/upload (SPHIN-21).
//!
//! Embedding shells out to the `exiftool` command-line tool (Phil Harvey's
//! Image::ExifTool) rather than reimplementing IPTC/XMP writing from
//! scratch -- it is the de facto standard for this, handles far more
//! formats and edge cases than a bespoke writer would, and keeps
//! sphinx-core out of the binary metadata-format business entirely. The
//! binary itself is not bundled; it must already be on `PATH`, or pointed
//! at explicitly via [`EmbedConfig::exiftool_path`] (packaging a bundled
//! copy is a packaging-epic concern, SPHIN-10).
//!
//! Argument construction ([`build_args`]) is pure and unit-tested offline;
//! no test in this module actually invokes exiftool.

mod csv_export;

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

pub use csv_export::{export_csv, ExportRow};

use crate::error::{CoreError, Result};
use crate::metadata::GeneratedMetadata;

/// Where to find the exiftool binary for a project. Empty path means "look
/// up `exiftool` on PATH".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedConfig {
    #[serde(default)]
    pub exiftool_path: String,
}

impl EmbedConfig {
    fn binary(&self) -> &str {
        let p = self.exiftool_path.trim();
        if p.is_empty() {
            "exiftool"
        } else {
            p
        }
    }
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            exiftool_path: String::new(),
        }
    }
}

/// Outcome of a successful embed. `updated` is false when exiftool ran
/// cleanly but reported no changes (e.g. the file already held identical
/// values), which is worth distinguishing from a genuine write in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedOutcome {
    pub updated: bool,
    pub message: String,
}

/// The exiftool CLI arguments that write `meta` into a file (everything
/// except the target path itself). List-type tags (`Keywords` / `Subject`)
/// are cleared with an empty assignment before the new values are added --
/// on a single exiftool invocation this replaces rather than appends, so
/// re-embedding after a re-generation doesn't leave stale keywords behind.
fn build_args(meta: &GeneratedMetadata) -> Vec<String> {
    let mut args = vec![
        "-overwrite_original".to_string(),
        "-codedcharacterset=utf8".to_string(),
        format!("-IPTC:ObjectName={}", meta.title),
        format!("-XMP-dc:Title={}", meta.title),
        format!("-IPTC:Caption-Abstract={}", meta.description),
        format!("-XMP-dc:Description={}", meta.description),
        "-IPTC:Keywords=".to_string(),
        "-XMP-dc:Subject=".to_string(),
    ];
    for kw in &meta.keywords {
        args.push(format!("-IPTC:Keywords={kw}"));
        args.push(format!("-XMP-dc:Subject={kw}"));
    }
    args
}

fn tool_error(e: std::io::Error) -> CoreError {
    CoreError::ExternalTool {
        tool: "exiftool".to_string(),
        message: format!("could not run exiftool ({e}); install it or set a custom exiftool path"),
    }
}

/// Write `meta` into the file at `path` as IPTC and XMP tags.
pub fn embed_metadata(
    path: impl AsRef<Path>,
    meta: &GeneratedMetadata,
    config: &EmbedConfig,
) -> Result<EmbedOutcome> {
    let output = Command::new(config.binary())
        .args(build_args(meta))
        .arg(path.as_ref())
        .output()
        .map_err(tool_error)?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        return Err(CoreError::ExternalTool {
            tool: "exiftool".to_string(),
            message: if stderr.is_empty() { stdout } else { stderr },
        });
    }

    Ok(EmbedOutcome {
        updated: stdout.contains("image files updated") && !stdout.contains("0 image files updated"),
        message: stdout,
    })
}

/// Whether exiftool is reachable, returning its version string if so. Lets
/// the UI show a clear "not installed" hint up front rather than failing on
/// the first real embed attempt.
pub fn check_exiftool(config: &EmbedConfig) -> Result<String> {
    let output = Command::new(config.binary())
        .arg("-ver")
        .output()
        .map_err(tool_error)?;
    if !output.status.success() {
        return Err(CoreError::ExternalTool {
            tool: "exiftool".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_meta() -> GeneratedMetadata {
        GeneratedMetadata {
            title: "A dog in a park".to_string(),
            description: "A golden retriever runs across a sunlit park.".to_string(),
            keywords: vec!["dog".to_string(), "park".to_string()],
            profile: "Shutterstock".to_string(),
            meets_minimum_keywords: true,
        }
    }

    #[test]
    fn args_include_title_description_and_each_keyword() {
        let args = build_args(&sample_meta());
        assert!(args.contains(&"-IPTC:ObjectName=A dog in a park".to_string()));
        assert!(args.contains(&"-XMP-dc:Title=A dog in a park".to_string()));
        assert!(args.contains(&"-IPTC:Caption-Abstract=A golden retriever runs across a sunlit park.".to_string()));
        assert!(args.contains(&"-IPTC:Keywords=dog".to_string()));
        assert!(args.contains(&"-IPTC:Keywords=park".to_string()));
        assert!(args.contains(&"-XMP-dc:Subject=dog".to_string()));
        assert!(args.contains(&"-XMP-dc:Subject=park".to_string()));
    }

    #[test]
    fn list_tags_are_cleared_before_being_repopulated() {
        let args = build_args(&sample_meta());
        let clear_pos = args.iter().position(|a| a == "-IPTC:Keywords=").unwrap();
        let first_kw_pos = args.iter().position(|a| a == "-IPTC:Keywords=dog").unwrap();
        assert!(clear_pos < first_kw_pos);
    }

    #[test]
    fn overwrite_original_is_always_set() {
        assert!(build_args(&sample_meta()).contains(&"-overwrite_original".to_string()));
    }

    #[test]
    fn empty_path_falls_back_to_exiftool_on_path() {
        assert_eq!(EmbedConfig::default().binary(), "exiftool");
        let mut cfg = EmbedConfig::default();
        cfg.exiftool_path = "  ".to_string();
        assert_eq!(cfg.binary(), "exiftool");
    }

    #[test]
    fn custom_path_is_used_verbatim() {
        let cfg = EmbedConfig {
            exiftool_path: "C:\\Tools\\exiftool.exe".to_string(),
        };
        assert_eq!(cfg.binary(), "C:\\Tools\\exiftool.exe");
    }

    #[test]
    fn missing_binary_is_an_external_tool_error() {
        let cfg = EmbedConfig {
            exiftool_path: "definitely-not-a-real-binary-xyz".to_string(),
        };
        let err = embed_metadata("nonexistent.jpg", &sample_meta(), &cfg).unwrap_err();
        assert!(matches!(err, CoreError::ExternalTool { .. }));
    }
}
