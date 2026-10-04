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
use crate::models::MediaType;

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
///
/// IPTC is a still-image metadata format and isn't defined for video
/// containers (mp4/mov/etc): exiftool accepts `-IPTC:*` for a video file
/// without erroring, but silently drops it, which used to leave title,
/// description and keywords invisible in every video-aware tool even
/// though the embed reported success. Videos instead get title/description
/// written to the QuickTime `Keys` atom group (Apple's `com.apple.quicktime.*`
/// asset metadata, read by Finder, QuickTime Player and most media/stock
/// tools) -- but that key set has no "keywords" entry, and neither the
/// legacy `UserData:Keywords` atom (`udta/kywd`, read by QuickTime-family
/// tools) nor XMP-dc:Subject is what Windows Explorer's Properties > Details
/// "Tags" field reads for mp4/mov. Confirmed by hand (manually setting Tags
/// on a video from Explorer's own Properties dialog, then inspecting the
/// result with exiftool): Explorer stores that field in the `udta/Xtra`
/// box's `WM/Category` attribute, not `WM/Keywords` -- exiftool exposes it
/// as `Microsoft:Category`, so keywords are written there too.
fn build_args(meta: &GeneratedMetadata, media_type: MediaType) -> Vec<String> {
    let mut args = vec![
        "-overwrite_original".to_string(),
        "-codedcharacterset=utf8".to_string(),
        format!("-XMP-dc:Title={}", meta.title),
        format!("-XMP-dc:Description={}", meta.description),
        "-XMP-dc:Subject=".to_string(),
    ];
    match media_type {
        MediaType::Image => {
            args.push(format!("-IPTC:ObjectName={}", meta.title));
            args.push(format!("-IPTC:Caption-Abstract={}", meta.description));
            args.push("-IPTC:Keywords=".to_string());
        }
        MediaType::Video => {
            args.push(format!("-Keys:Title={}", meta.title));
            args.push(format!("-Keys:Description={}", meta.description));
            args.push("-UserData:Keywords=".to_string());
            args.push("-Microsoft:Category=".to_string());
        }
    }
    for kw in &meta.keywords {
        args.push(format!("-XMP-dc:Subject={kw}"));
        match media_type {
            MediaType::Image => args.push(format!("-IPTC:Keywords={kw}")),
            MediaType::Video => {
                args.push(format!("-UserData:Keywords={kw}"));
                args.push(format!("-Microsoft:Category={kw}"));
            }
        }
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
    let media_type = path
        .as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .and_then(MediaType::from_extension)
        .unwrap_or(MediaType::Image);

    let output = Command::new(config.binary())
        .args(build_args(meta, media_type))
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
        let args = build_args(&sample_meta(), MediaType::Image);
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
        let args = build_args(&sample_meta(), MediaType::Image);
        let clear_pos = args.iter().position(|a| a == "-IPTC:Keywords=").unwrap();
        let first_kw_pos = args.iter().position(|a| a == "-IPTC:Keywords=dog").unwrap();
        assert!(clear_pos < first_kw_pos);
    }

    #[test]
    fn overwrite_original_is_always_set() {
        assert!(build_args(&sample_meta(), MediaType::Image).contains(&"-overwrite_original".to_string()));
    }

    #[test]
    fn video_args_use_quicktime_keys_instead_of_iptc() {
        let args = build_args(&sample_meta(), MediaType::Video);
        assert!(!args.iter().any(|a| a.starts_with("-IPTC:")));
        assert!(args.contains(&"-Keys:Title=A dog in a park".to_string()));
        assert!(args.contains(&"-Keys:Description=A golden retriever runs across a sunlit park.".to_string()));
        assert!(args.contains(&"-UserData:Keywords=dog".to_string()));
        assert!(args.contains(&"-UserData:Keywords=park".to_string()));
        assert!(args.contains(&"-Microsoft:Category=dog".to_string()));
        assert!(args.contains(&"-Microsoft:Category=park".to_string()));
        assert!(args.contains(&"-XMP-dc:Subject=dog".to_string()));
        assert!(args.contains(&"-XMP-dc:Subject=park".to_string()));
    }

    #[test]
    fn video_list_tags_are_cleared_before_being_repopulated() {
        let args = build_args(&sample_meta(), MediaType::Video);
        let clear_pos = args.iter().position(|a| a == "-UserData:Keywords=").unwrap();
        let first_kw_pos = args.iter().position(|a| a == "-UserData:Keywords=dog").unwrap();
        assert!(clear_pos < first_kw_pos);

        let ms_clear_pos = args.iter().position(|a| a == "-Microsoft:Category=").unwrap();
        let ms_first_kw_pos = args.iter().position(|a| a == "-Microsoft:Category=dog").unwrap();
        assert!(ms_clear_pos < ms_first_kw_pos);
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
