//! Parse Squad UE5 replay data into typed bundles.
//!
//! **squadreplay** is a library-first parser for
//! [Squad](https://joinsquad.com/) UE5 `.replay` files. It extracts teams,
//! squads, players, kills, vehicle tracks, deployments, and more into a
//! single [`Bundle`] struct that can be serialized to JSON (`.sqrj.json`) or
//! binary MessagePack (`.sqrb`) for downstream tooling.
//!
//! # Quick start
//!
//! ```no_run
//! use squadreplay::{parse_file, ParseOptions};
//!
//! let bundle = parse_file("match.replay", &ParseOptions::default())?;
//! println!("map: {:?}", bundle.replay.map_name);
//! println!("players: {}", bundle.players.len());
//! println!("kills: {}", bundle.events.kills.len());
//! # Ok::<(), squadreplay::Error>(())
//! ```
//!
//! # Serialization round-trip
//!
//! ```no_run
//! use squadreplay::{parse_file, sqrb, sqrj, ParseOptions};
//!
//! let bundle = parse_file("match.replay", &ParseOptions::default())?;
//!
//! // Write to both formats
//! sqrj::write(&bundle, "match.sqrj.json")?;
//! sqrb::write(&bundle, "match.sqrb")?;
//!
//! // Read back
//! let from_json = sqrj::read("match.sqrj.json")?;
//! let from_bin  = sqrb::read("match.sqrb")?;
//! # Ok::<(), squadreplay::Error>(())
//! ```
//!
//! # Feature flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `cli`   | yes     | Builds the `squadreplay` binary (pulls in [`clap`](https://docs.rs/clap)) |
//!
//! Library-only consumers should disable default features:
//!
//! ```toml
//! [dependencies]
//! squadreplay = { version = "0.1.0-alpha.1", default-features = false }
//! ```

#![warn(missing_docs)]

use std::path::Path;

/// Data model for parsed Squad replay bundles.
///
/// All top-level types ([`Bundle`], [`Team`](bundle::Team),
/// [`Player`](bundle::Player), etc.) live in this module.
#[path = "model.rs"]
pub mod bundle;
mod classify;
/// Compatibility layer producing the legacy JSON format used by older Squad
/// replay tools.
pub mod compat;
mod error;
mod formats;
/// Parser for Squad game log files (SquadGame.log).
pub mod log_parser;
mod parser;
/// Read and write `.sqrb` (binary MessagePack) bundles.
pub mod sqrb;
/// Read and write `.sqrj.json` (human-readable JSON) bundles.
pub mod sqrj;
/// Ticket proof format for match analysis.
pub mod ticketproof;
/// Timeline format for Squad Replay Viewer (`.sqrt.json`).
pub mod timeline;
mod unreal_names;

pub use bundle::{Bundle, GameStateInfo, ParseOptions};
pub use error::{Error, Result};

/// Parse a replay file from disk.
pub fn parse_file(path: impl AsRef<Path>, options: &ParseOptions) -> Result<Bundle> {
    let path = path.as_ref();
    let mut bundle = parser::parse_file(path, options.include_property_events)?;
    
    // Merge log data if provided
    if let Some(log_path) = &options.log_path {
        if log_path.exists() {
            if let Ok(log_matches) = log_parser::parse_log_file(log_path) {
                let replay_duration = bundle.replay.duration_ms;
                
                // Get replay filename for matching (without extension)
                let replay_name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.trim_end_matches(".replay"));
                
                // Get replay file modification time for matching (in local time to match log timestamps)
                let replay_end_time = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| {
                        let datetime: chrono::DateTime<chrono::Local> = t.into();
                        datetime.naive_local()
                    });
                
                if let Some(log_match) = log_parser::find_matching_log_by_name(&log_matches, replay_duration, replay_end_time, options.tz_offset_hours, replay_name) {
                    log_parser::merge_log_into_players(&mut bundle.players, log_match, replay_duration);
                    merge_vehicle_deaths(&mut bundle.events.vehicle_deaths, log_match);
                }
            }
        }
    }
    
    Ok(bundle)
}

/// Merge vehicle death events from log into bundle.
fn merge_vehicle_deaths(vehicle_deaths: &mut Vec<bundle::VehicleDeathEvent>, log_match: &log_parser::LogMatch) {
    for event in &log_match.events {
        if let log_parser::LogEventType::VehicleDied { vehicle_class, vehicle_id, causer, instigator } = &event.event_type {
            vehicle_deaths.push(bundle::VehicleDeathEvent {
                t_ms: event.t_ms,
                second: (event.t_ms / 1000) as u32,
                vehicle_class: vehicle_class.clone(),
                vehicle_id: vehicle_id.clone(),
                causer: causer.clone(),
                instigator: instigator.clone(),
            });
        }
    }
}

/// Parse replay bytes that are already loaded in memory.
pub fn parse_bytes(
    bytes: impl AsRef<[u8]>,
    file_name: Option<String>,
    options: &ParseOptions,
) -> Result<Bundle> {
    let mut bundle = parser::parse_bytes(bytes.as_ref(), file_name.clone(), options.include_property_events)?;
    
    // Merge log data if provided
    if let Some(log_path) = &options.log_path {
        if log_path.exists() {
            if let Ok(log_matches) = log_parser::parse_log_file(log_path) {
                let replay_duration = bundle.replay.duration_ms;
                
                // Get replay name from file_name if provided
                let replay_name = file_name.as_ref().map(|s| s.trim_end_matches(".replay"));
                
                // No file to get modification time from - will use duration fallback
                if let Some(log_match) = log_parser::find_matching_log_by_name(&log_matches, replay_duration, None, options.tz_offset_hours, replay_name) {
                    log_parser::merge_log_into_players(&mut bundle.players, log_match, replay_duration);
                    merge_vehicle_deaths(&mut bundle.events.vehicle_deaths, log_match);
                }
            }
        }
    }
    
    Ok(bundle)
}

/// Read a serialized bundle from disk.
///
/// The format is inferred from the file extension.
pub fn read_bundle(path: impl AsRef<Path>) -> Result<Bundle> {
    let path = path.as_ref();
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if name.ends_with(".sqrb") {
        return sqrb::read(path);
    }
    if name.ends_with(".sqrj") || name.ends_with(".sqrj.json") || name.ends_with(".json") {
        return sqrj::read(path);
    }

    Err(Error::Unsupported(format!(
        "cannot infer bundle format from `{}`; expected .sqrb or .sqrj.json",
        path.display()
    )))
}

/// Re-exports of internal types for benchmark-only access.
///
/// This module is gated behind the `bench-internals` Cargo feature and is
/// **not** part of the public API.  It exists solely so that Criterion
/// benchmarks in `benches/classify.rs` can exercise the classification
/// hot-path without making the `classify` module permanently public.
#[cfg(feature = "bench-internals")]
pub mod bench_internals {
    pub use crate::classify::{
        ClassifyFlags, classify_deployable_event_type, infer_component_type_name,
        infer_group_leaf, is_deployable_primary_type, is_helicopter_type, is_soldier_type,
        is_vehicle_type, normalize_type,
    };
}
