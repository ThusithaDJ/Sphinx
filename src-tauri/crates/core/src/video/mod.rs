//! SPHIN-31: scene detection and keyframe extraction for video, plus optional
//! audio extraction feeding SPHIN-32's transcription.
//!
//! Shells out to `ffmpeg` rather than pulling in a Rust video-decoding crate,
//! for the same reason [`crate::embed`] shells out to `exiftool`: ffmpeg is
//! the de facto standard, handles far more container/codec combinations than
//! a bespoke decoder would, and keeps sphinx-core out of the codec business
//! entirely. Not bundled; must be on `PATH` or pointed at explicitly via
//! [`VideoConfig::ffmpeg_path`] (bundling is a packaging-epic concern,
//! SPHIN-10).
//!
//! Argument construction ([`build_keyframe_args`], [`build_audio_args`]) is
//! pure and unit-tested offline; no test in this module actually invokes
//! ffmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Where to find ffmpeg for a project, plus keyframe-extraction tuning.
/// Empty `ffmpeg_path` means "look up `ffmpeg` on PATH".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    #[serde(default)]
    pub ffmpeg_path: String,
    /// Maximum number of keyframes to extract per video.
    #[serde(default = "default_max_keyframes")]
    pub max_keyframes: usize,
    /// ffmpeg `scene` score (0.0-1.0) above which a frame counts as a scene
    /// change. Lower = more frames extracted.
    #[serde(default = "default_scene_threshold")]
    pub scene_threshold: f32,
}

fn default_max_keyframes() -> usize {
    6
}

fn default_scene_threshold() -> f32 {
    0.4
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: String::new(),
            max_keyframes: default_max_keyframes(),
            scene_threshold: default_scene_threshold(),
        }
    }
}

impl VideoConfig {
    fn binary(&self) -> &str {
        let p = self.ffmpeg_path.trim();
        if p.is_empty() {
            "ffmpeg"
        } else {
            p
        }
    }
}

fn tool_error(e: std::io::Error) -> CoreError {
    CoreError::ExternalTool {
        tool: "ffmpeg".to_string(),
        message: format!("could not run ffmpeg ({e}); install it or set a custom ffmpeg path"),
    }
}

/// Build the ffmpeg arguments that sample up to `max_frames` keyframes from
/// `video_path` into `out_pattern` (an ffmpeg `%03d`-style path template).
/// Frame 0 is always selected (`eq(n,0)`) in addition to detected scene
/// changes, so a static video with no scene changes still yields one
/// representative frame rather than zero.
fn build_keyframe_args(
    video_path: &str,
    out_pattern: &str,
    threshold: f32,
    max_frames: usize,
) -> Vec<String> {
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-y".to_string(),
        "-i".to_string(),
        video_path.to_string(),
        "-vf".to_string(),
        format!("select='eq(n\\,0)+gt(scene\\,{threshold})'"),
        "-fps_mode".to_string(),
        "vfr".to_string(),
        "-frames:v".to_string(),
        max_frames.to_string(),
        "-q:v".to_string(),
        "2".to_string(),
        out_pattern.to_string(),
    ]
}

/// Build the ffmpeg arguments that extract `video_path`'s audio track as a
/// 16kHz mono WAV at `out_path` -- the format Whisper-compatible APIs expect.
fn build_audio_args(video_path: &str, out_path: &str) -> Vec<String> {
    vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-y".to_string(),
        "-i".to_string(),
        video_path.to_string(),
        "-vn".to_string(),
        "-ac".to_string(),
        "1".to_string(),
        "-ar".to_string(),
        "16000".to_string(),
        out_path.to_string(),
    ]
}

/// Whether ffmpeg is reachable, returning its version string if so.
pub fn check_ffmpeg(config: &VideoConfig) -> Result<String> {
    let output = Command::new(config.binary())
        .arg("-version")
        .output()
        .map_err(tool_error)?;
    if !output.status.success() {
        return Err(CoreError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().next().unwrap_or("").trim().to_string())
}

/// Sample keyframes from `video_path` into `out_dir` (which must already
/// exist), returning their paths in chronological order.
pub fn extract_keyframes(
    config: &VideoConfig,
    video_path: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
) -> Result<Vec<PathBuf>> {
    let out_pattern = out_dir.as_ref().join("frame_%03d.jpg");
    let args = build_keyframe_args(
        &video_path.as_ref().to_string_lossy(),
        &out_pattern.to_string_lossy(),
        config.scene_threshold,
        config.max_keyframes,
    );

    let output = Command::new(config.binary())
        .args(&args)
        .output()
        .map_err(tool_error)?;

    if !output.status.success() {
        return Err(CoreError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let mut frames: Vec<PathBuf> = std::fs::read_dir(out_dir.as_ref())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jpg"))
        .collect();
    frames.sort();

    if frames.is_empty() {
        return Err(CoreError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: "ffmpeg produced no keyframes for this video".to_string(),
        });
    }

    Ok(frames)
}

/// Extract `video_path`'s audio track to `out_path` as a 16kHz mono WAV, for
/// [`crate::transcribe`]. Fails (e.g. the video has no audio stream) rather
/// than silently producing an empty file -- callers that treat transcription
/// as optional should catch and ignore the error.
pub fn extract_audio(
    config: &VideoConfig,
    video_path: impl AsRef<Path>,
    out_path: impl AsRef<Path>,
) -> Result<()> {
    let args = build_audio_args(
        &video_path.as_ref().to_string_lossy(),
        &out_path.as_ref().to_string_lossy(),
    );

    let output = Command::new(config.binary())
        .args(&args)
        .output()
        .map_err(tool_error)?;

    if !output.status.success() {
        return Err(CoreError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_args_always_include_frame_zero_and_the_scene_filter() {
        let args = build_keyframe_args("in.mp4", "out/frame_%03d.jpg", 0.4, 6);
        let vf_pos = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_pos + 1], "select='eq(n\\,0)+gt(scene\\,0.4)'");
        assert!(args.contains(&"-frames:v".to_string()));
        assert!(args.contains(&"6".to_string()));
        assert_eq!(args.last().unwrap(), "out/frame_%03d.jpg");
    }

    #[test]
    fn keyframe_args_use_fps_mode_not_the_removed_vsync_flag() {
        // -vsync was removed from recent ffmpeg builds in favor of -fps_mode;
        // using the old flag fails with "Unrecognized option 'vsync'".
        let args = build_keyframe_args("in.mp4", "out/frame_%03d.jpg", 0.4, 6);
        assert!(!args.contains(&"-vsync".to_string()));
        let pos = args.iter().position(|a| a == "-fps_mode").unwrap();
        assert_eq!(args[pos + 1], "vfr");
    }

    #[test]
    fn keyframe_args_respect_a_custom_threshold_and_frame_cap() {
        let args = build_keyframe_args("in.mov", "out.jpg", 0.15, 12);
        let vf_pos = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_pos + 1], "select='eq(n\\,0)+gt(scene\\,0.15)'");
        let frames_pos = args.iter().position(|a| a == "-frames:v").unwrap();
        assert_eq!(args[frames_pos + 1], "12");
    }

    #[test]
    fn audio_args_downmix_to_16khz_mono() {
        let args = build_audio_args("in.mp4", "out.wav");
        assert!(args.contains(&"-vn".to_string()));
        let ac_pos = args.iter().position(|a| a == "-ac").unwrap();
        assert_eq!(args[ac_pos + 1], "1");
        let ar_pos = args.iter().position(|a| a == "-ar").unwrap();
        assert_eq!(args[ar_pos + 1], "16000");
        assert_eq!(args.last().unwrap(), "out.wav");
    }

    #[test]
    fn empty_path_falls_back_to_ffmpeg_on_path() {
        assert_eq!(VideoConfig::default().binary(), "ffmpeg");
        let mut cfg = VideoConfig::default();
        cfg.ffmpeg_path = "  ".to_string();
        assert_eq!(cfg.binary(), "ffmpeg");
    }

    #[test]
    fn custom_path_is_used_verbatim() {
        let cfg = VideoConfig {
            ffmpeg_path: "C:\\Tools\\ffmpeg.exe".to_string(),
            ..VideoConfig::default()
        };
        assert_eq!(cfg.binary(), "C:\\Tools\\ffmpeg.exe");
    }

    #[test]
    fn defaults_are_sensible() {
        let cfg = VideoConfig::default();
        assert_eq!(cfg.max_keyframes, 6);
        assert!((cfg.scene_threshold - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn missing_binary_is_an_external_tool_error() {
        let cfg = VideoConfig {
            ffmpeg_path: "definitely-not-a-real-binary-xyz".to_string(),
            ..VideoConfig::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let err = extract_keyframes(&cfg, "nonexistent.mp4", dir.path()).unwrap_err();
        assert!(matches!(err, CoreError::ExternalTool { .. }));
    }
}
