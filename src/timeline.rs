//! Timeline output format for the Squad Replay Viewer.
//!
//! Produces a compact, viewer-ready `.sqrt.json` file that contains all data
//! needed to visualize a match: players, vehicles, capture zones, deployables,
//! and time-series position tracks.

use crate::bundle::{
    ActorEntity, Bundle, CaptureZone, KillEvent, Player, Squad, Team, Track3, TrackSample3,
    VisibilityWindow,
};
use crate::classify::is_helicopter_type;
use crate::log_parser::LogMatch;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Timeline format version
pub const TIMELINE_VERSION: u32 = 1;

/// Default sample interval in milliseconds (333ms = ~3 samples/second)
pub const DEFAULT_SAMPLE_INTERVAL_MS: u64 = 333;

const MIN_WORLD_COORD_ABS: f64 = 100.0;

// ============================================================================
// Timeline data structures
// ============================================================================

/// Root timeline structure for `.sqrt.json` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub version: u32,
    pub r#match: MatchInfo,
    pub teams: Vec<TimelineTeam>,
    pub squads: Vec<TimelineSquad>,
    pub players: BTreeMap<String, TimelinePlayer>,
    pub tracks: TimelineTracks,
    pub capture_zones: Vec<TimelineCaptureZone>,
    pub deployables: Vec<TimelineDeployable>,
    pub events: TimelineEvents,
}

/// Match metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    pub map_name: Option<String>,
    pub layer_name: Option<String>,
    pub duration_ms: u64,
    pub started_at: Option<String>,
    pub server_name: Option<String>,
    pub game_mode: Option<String>,
}

/// Team information with normalized IDs (1 or 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTeam {
    pub id: u32,
    pub faction: Option<String>,
    pub name: Option<String>,
}

/// Squad information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSquad {
    pub id: u32,
    pub name: Option<String>,
    pub team_id: u32,
    pub leader_eos: Option<String>,
}

/// Player information keyed by EOS ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePlayer {
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub team_id: Option<u32>,
    pub squad_id: Option<u32>,
    pub visibility_windows: Vec<[u64; 2]>,
}

/// All position tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineTracks {
    pub players: BTreeMap<String, Vec<TimelinePositionSample>>,
    pub vehicles: BTreeMap<String, TimelineVehicle>,
}

/// A single position sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePositionSample {
    pub t: u64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f64>,
}

/// Vehicle with position track and seat events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineVehicle {
    pub class: String,
    pub r#type: String, // "ground" or "helicopter"
    pub team_id: Option<u32>,
    pub samples: Vec<TimelinePositionSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seat_events: Vec<TimelineSeatEvent>,
}

/// Vehicle seat occupancy event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSeatEvent {
    pub t: u64,
    pub seat: String,
    pub player_eos: Option<String>,
    pub entering: bool,
}

/// Capture zone with ownership events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCaptureZone {
    pub name: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub events: Vec<TimelineCaptureEvent>,
}

/// Capture zone ownership change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCaptureEvent {
    pub t: u64,
    pub owner: Option<u32>,
}

/// Deployable health change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineHealthEvent {
    pub t: u64,
    pub health: f64,
}

/// Deployable ammo change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineAmmoEvent {
    pub t: u64,
    pub ammo: f64,
}

/// Deployable construction points change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineConstructionEvent {
    pub t: u64,
    pub points: f64,
}

/// Deployable object (FOB, HAB, rally, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDeployable {
    pub class: String,
    pub team_id: Option<u32>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub placed_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destroyed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub health_events: Vec<TimelineHealthEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ammo_events: Vec<TimelineAmmoEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub construction_events: Vec<TimelineConstructionEvent>,
}

/// Game events (kills, spawns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvents {
    pub kills: Vec<TimelineKillEvent>,
    pub spawns: Vec<TimelineSpawnEvent>,
}

/// Kill event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineKillEvent {
    pub t: u64,
    pub victim: Option<String>,
    pub killer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_incap: Option<bool>,
}

/// Spawn event with kit info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSpawnEvent {
    pub t: u64,
    pub player: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kit: Option<String>,
}

// ============================================================================
// Timeline builder
// ============================================================================

/// Options for building a timeline.
#[derive(Debug, Clone)]
pub struct TimelineOptions {
    /// Sample interval in milliseconds (default: 333ms = 3 samples/sec)
    pub sample_interval_ms: u64,
}

impl Default for TimelineOptions {
    fn default() -> Self {
        Self {
            sample_interval_ms: DEFAULT_SAMPLE_INTERVAL_MS,
        }
    }
}

/// Build a timeline from a parsed bundle.
pub fn build_timeline(
    bundle: &Bundle,
    _log_match: Option<&LogMatch>,
    options: &TimelineOptions,
) -> Timeline {
    // Build EOS ID to player info lookup
    let eos_to_player: HashMap<String, &Player> = bundle
        .players
        .iter()
        .filter_map(|p| p.eos_id.as_ref().map(|eos| (eos.to_lowercase(), p)))
        .collect();

    // Build player state GUID to EOS ID lookup
    let guid_to_eos: HashMap<u32, String> = bundle
        .players
        .iter()
        .filter_map(|p| {
            p.eos_id.as_ref().map(|eos| (p.player_state_guid, eos.to_lowercase()))
        })
        .collect();

    // Build name to EOS ID lookup (for track key resolution)
    let name_to_eos: HashMap<String, String> = bundle
        .players
        .iter()
        .filter_map(|p| {
            p.eos_id.as_ref().and_then(|eos| {
                p.name.as_ref().map(|name| (name.to_lowercase(), eos.to_lowercase()))
            })
        })
        .collect();

    Timeline {
        version: TIMELINE_VERSION,
        r#match: build_match_info(bundle),
        teams: build_teams(&bundle.teams),
        squads: build_squads(&bundle.squads, &eos_to_player),
        players: build_players(&bundle.players),
        tracks: build_tracks(bundle, &guid_to_eos, &name_to_eos, options),
        capture_zones: build_capture_zones(&bundle.events.capture_zones),
        deployables: build_deployables(bundle),
        events: build_events(bundle, &eos_to_player),
    }
}

fn build_match_info(bundle: &Bundle) -> MatchInfo {
    // Extract game mode from layer name or game state
    let game_mode = bundle
        .game_state
        .game_mode
        .clone()
        .or_else(|| {
            bundle.replay.layer_name.as_ref().and_then(|layer| {
                // Extract mode from layer name like "Manicouagan_Seed_v2_CL"
                let parts: Vec<&str> = layer.split('_').collect();
                parts.get(1).map(|s| s.to_string())
            })
        });

    MatchInfo {
        map_name: bundle.replay.map_name.clone(),
        layer_name: bundle.replay.layer_name.clone(),
        duration_ms: bundle.replay.duration_ms,
        started_at: bundle.replay.started_at.clone(),
        server_name: bundle.game_state.server_name.clone(),
        game_mode,
    }
}

fn build_teams(teams: &[Team]) -> Vec<TimelineTeam> {
    teams
        .iter()
        .map(|t| TimelineTeam {
            id: normalize_team_id(t.id),
            faction: t.faction.clone(),
            name: t.name.clone(),
        })
        .collect()
}

fn build_squads(squads: &[Squad], eos_to_player: &HashMap<String, &Player>) -> Vec<TimelineSquad> {
    squads
        .iter()
        .map(|s| {
            // Try to find leader EOS ID from player state GUID
            let leader_eos = s.leader_eos_id.clone().or_else(|| {
                s.leader_steam_id.as_ref().and_then(|steam| {
                    // Find player with matching steam ID
                    eos_to_player.values()
                        .find(|p| p.steam_id.as_ref() == Some(steam))
                        .and_then(|p| p.eos_id.clone())
                })
            });

            TimelineSquad {
                id: s.id,
                name: s.name.clone(),
                team_id: normalize_team_id(s.team_id.unwrap_or(s.raw_team_id.unwrap_or(1))),
                leader_eos,
            }
        })
        .collect()
}

fn build_players(players: &[Player]) -> BTreeMap<String, TimelinePlayer> {
    let mut result = BTreeMap::new();

    for player in players {
        // Use EOS ID if available, otherwise use lowercased name as key
        // This matches how track keys are assigned in build_player_tracks
        let key = player
            .eos_id
            .as_ref()
            .map(|id| id.to_lowercase())
            .or_else(|| player.name.as_ref().map(|n| n.to_lowercase()));
        
        let key = match key {
            Some(k) => k,
            None => continue, // Skip players with neither EOS ID nor name
        };

        let visibility_windows: Vec<[u64; 2]> = player
            .visibility_windows
            .iter()
            .map(|w| [w.start_ms, w.end_ms])
            .collect();

        result.insert(
            key,
            TimelinePlayer {
                name: player.name.clone(),
                steam_id: player.steam_id.clone(),
                team_id: player.team_id.map(normalize_team_id),
                squad_id: player.squad_id,
                visibility_windows,
            },
        );
    }

    result
}

fn build_tracks(
    bundle: &Bundle,
    guid_to_eos: &HashMap<u32, String>,
    name_to_eos: &HashMap<String, String>,
    options: &TimelineOptions,
) -> TimelineTracks {
    TimelineTracks {
        players: build_player_tracks(&bundle.tracks.players, guid_to_eos, name_to_eos, options),
        vehicles: build_vehicle_tracks(bundle, guid_to_eos, options),
    }
}

fn build_player_tracks(
    tracks: &[Track3],
    guid_to_eos: &HashMap<u32, String>,
    name_to_eos: &HashMap<String, String>,
    options: &TimelineOptions,
) -> BTreeMap<String, Vec<TimelinePositionSample>> {
    let mut result = BTreeMap::new();

    for track in tracks {
        // Get EOS ID from:
        // 1. player state GUID lookup
        // 2. track name lookup (for players whose tracks don't have GUIDs)
        // 3. fallback to lowercased track key (for players without EOS IDs)
        let eos_id = track
            .player_state_guid
            .and_then(|guid| guid_to_eos.get(&guid).cloned())
            .or_else(|| name_to_eos.get(&track.key.to_lowercase()).cloned())
            .unwrap_or_else(|| track.key.to_lowercase());

        let samples = downsample_track(&track.samples, options.sample_interval_ms);

        if !samples.is_empty() {
            result.insert(eos_id, samples);
        }
    }

    result
}

fn build_vehicle_tracks(
    bundle: &Bundle,
    guid_to_eos: &HashMap<u32, String>,
    options: &TimelineOptions,
) -> BTreeMap<String, TimelineVehicle> {
    let mut result = BTreeMap::new();
    let mut inserted_actor_guids = std::collections::HashSet::new();

    // Owner actor fallback data for vehicles that never emit direct movement.
    let owner_team_hints = build_owner_team_hints(bundle);
    let team_aliases = build_team_aliases(bundle);
    let child_owner_hints = build_child_owner_hints(bundle);
    let owner_player_states = build_owner_player_state_windows(bundle);
    let player_name_by_state = build_player_name_by_state(bundle);
    let player_tracks_by_name = build_player_tracks_by_name(bundle);
    let track_lookup: HashMap<u32, &Track3> = bundle
        .tracks
        .vehicles
        .iter()
        .chain(bundle.tracks.helicopters.iter())
        .filter_map(|t| t.actor_guid.map(|guid| (guid, t)))
        .collect();

    // Combine vehicles and helicopters into one map
    let all_vehicles: Vec<(&ActorEntity, &[Track3], bool)> = vec![
        // Ground vehicles
        bundle.actors.vehicles.iter()
            .map(|v| (v, bundle.tracks.vehicles.as_slice(), false))
            .collect::<Vec<_>>(),
        // Helicopters
        bundle.actors.helicopters.iter()
            .map(|h| (h, bundle.tracks.helicopters.as_slice(), true))
            .collect::<Vec<_>>(),
    ]
    .into_iter()
    .flatten()
    .collect();

    for (actor, tracks, is_heli) in all_vehicles {
        let guid_str = actor.actor_guid.to_string();
        inserted_actor_guids.insert(actor.actor_guid);

        // Find matching track
        let track_samples = tracks
            .iter()
            .find(|t| t.actor_guid == Some(actor.actor_guid))
            .map(|t| &t.samples[..])
            .unwrap_or(&[]);

        // Extract class name
        let class = actor
            .class_name
            .as_ref()
            .map(|c| extract_vehicle_class(c))
            .unwrap_or_else(|| "Unknown".to_string());

        // Determine vehicle type
        let vehicle_type = if is_heli || is_helicopter_type(&class) {
            "helicopter"
        } else {
            "ground"
        };

        // Build seat events from bundle seat changes
        let seat_events: Vec<TimelineSeatEvent> = bundle
            .events
            .seat_changes
            .iter()
            .filter(|s| s.actor_guid == Some(actor.actor_guid))
            .filter_map(|s| {
                // Determine if entering or exiting based on value
                let entering = s.value.as_ref().map(|v| !v.is_empty() && v != "None").unwrap_or(false);
                
                Some(TimelineSeatEvent {
                    t: s.t_ms,
                    seat: s.seat_attach_socket.clone().or(s.attach_socket_name.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    player_eos: s
                        .player_state_guid
                        .and_then(|guid| guid_to_eos.get(&guid).cloned()),
                    entering,
                })
            })
            .collect();

        let mut raw_samples = track_samples
            .iter()
            .map(|s| TrackSample3 {
                t_ms: s.t_ms,
                x: s.x,
                y: s.y,
                z: s.z,
                yaw: s.yaw,
            })
            .collect::<Vec<_>>();

        // Drop invalid origin samples; they are often uninitialized transforms.
        raw_samples.retain(|s| is_plausible_world_position(s.x, s.y));

        // Some vehicle actor classes (notably multi-part vehicles with turret-actor
        // ownership) do not replicate movement directly. Reconstruct approximate motion
        // from the currently-occupied player's track windows tied to the owner actor.
        let resolved_owner_guid = actor
            .owner
            .or_else(|| child_owner_hints.get(&actor.actor_guid).copied());

        if raw_samples.len() <= 1 {
            if let Some(owner_guid) = resolved_owner_guid {
                let inferred = infer_vehicle_samples_from_owner_player_state(
                    owner_guid,
                    &owner_player_states,
                    &player_name_by_state,
                    &player_tracks_by_name,
                );
                if inferred.len() > raw_samples.len() {
                    raw_samples = inferred;
                }
            }
        }

        // Some alive hull actors never emit direct ReplicatedMovement in
        // certain replays. Render them at least once from open transform.
        if raw_samples.is_empty() {
            if let Some(location) = actor.initial_location {
                if is_plausible_world_position(location.x, location.y) {
                    raw_samples.push(TrackSample3 {
                        t_ms: actor.open_time_ms,
                        x: location.x,
                        y: location.y,
                        z: location.z,
                        yaw: actor.initial_rotation.map(|r| r.yaw),
                    });
                }
            }
        }

        let samples = downsample_track(&raw_samples, options.sample_interval_ms);
        let mut samples = samples;

        // Synthetic channel actors are first seen when movement appears. If their
        // first sample is late, pin an initial static point at t=0 so start-of-round
        // map state remains visible.
        if class.contains("Logi") {
            if let Some(first) = samples.first().cloned() {
                if first.t > 10_000 {
                    samples.insert(
                        0,
                        TimelinePositionSample {
                            t: 0,
                            x: first.x,
                            y: first.y,
                            z: first.z,
                            yaw: first.yaw,
                        },
                    );
                }
            }
        }

        let resolved_team_id = actor
            .team
            .map(|t| t as u32)
            .or_else(|| {
                resolved_owner_guid.and_then(|owner_guid| owner_team_hints.get(&owner_guid).copied())
            })
            .map(|raw| team_aliases.get(&raw).copied().unwrap_or(raw));

        result.insert(
            guid_str,
            TimelineVehicle {
                class,
                r#type: vehicle_type.to_string(),
                team_id: resolved_team_id.map(normalize_team_id),
                samples,
                seat_events,
            },
        );
    }

    // Some vehicle hull owners never open as actors, while child turret/weapon
    // actors do. Aggregate those child tracks into an owner-keyed synthetic hull.
    let owner_team_hints = build_owner_team_hints(bundle);
    let team_aliases = build_team_aliases(bundle);
    let mut btr_owner_samples: HashMap<u32, Vec<TrackSample3>> = HashMap::new();
    for actor in &bundle.actors.vehicles {
        let class_name = actor.class_name.as_deref().unwrap_or_default();
        if !class_name.contains("BTR4") {
            continue;
        }
        let Some(owner_guid) = actor.owner else {
            continue;
        };
        if inserted_actor_guids.contains(&owner_guid) {
            continue;
        }
        let Some(track) = track_lookup.get(&actor.actor_guid) else {
            if let Some(location) = actor.initial_location {
                if is_plausible_world_position(location.x, location.y) {
                    btr_owner_samples
                        .entry(owner_guid)
                        .or_default()
                        .push(TrackSample3 {
                            t_ms: actor.open_time_ms,
                            x: location.x,
                            y: location.y,
                            z: location.z,
                            yaw: actor.initial_rotation.map(|r| r.yaw),
                        });
                }
            }
            continue;
        };
        if track.samples.is_empty() {
            continue;
        }
        btr_owner_samples
            .entry(owner_guid)
            .or_default()
            .extend(track.samples.iter().cloned());
    }

    for (owner_guid, mut owner_samples) in btr_owner_samples {
        owner_samples.sort_by_key(|s| s.t_ms);
        owner_samples.dedup_by_key(|s| s.t_ms);
        owner_samples.retain(|s| is_plausible_world_position(s.x, s.y));
        if owner_samples.is_empty() {
            continue;
        }

        let resolved_team = owner_team_hints
            .get(&owner_guid)
            .copied()
            .map(|raw| team_aliases.get(&raw).copied().unwrap_or(raw));

        result.entry(owner_guid.to_string()).or_insert(TimelineVehicle {
            class: "BTR4".to_string(),
            r#type: "ground".to_string(),
            team_id: resolved_team.map(normalize_team_id),
            samples: downsample_track(&owner_samples, options.sample_interval_ms),
            seat_events: Vec::new(),
        });
    }

    // Emit track-backed vehicles whose actor entities are missing from actor groups.
    for track in &bundle.tracks.vehicles {
        let Some(actor_guid) = track.actor_guid else {
            continue;
        };
        if result.contains_key(&actor_guid.to_string()) {
            continue;
        }

        let mut samples = track.samples.clone();
        samples.retain(|s| is_plausible_world_position(s.x, s.y));
        if samples.is_empty() {
            continue;
        }

        let class = track
            .class_name
            .as_ref()
            .map(|c| extract_vehicle_class(c))
            .unwrap_or_else(|| "Unknown".to_string());
        let resolved_team = owner_team_hints
            .get(&actor_guid)
            .copied()
            .map(|raw| team_aliases.get(&raw).copied().unwrap_or(raw));

        result.insert(
            actor_guid.to_string(),
            TimelineVehicle {
                class,
                r#type: "ground".to_string(),
                team_id: resolved_team.map(normalize_team_id),
                samples: downsample_track(&samples, options.sample_interval_ms),
                seat_events: Vec::new(),
            },
        );
    }

    result
}

fn is_plausible_world_position(x: f64, y: f64) -> bool {
    x.abs() > MIN_WORLD_COORD_ABS || y.abs() > MIN_WORLD_COORD_ABS
}

fn build_child_owner_hints(bundle: &Bundle) -> HashMap<u32, u32> {
    let mut hints = HashMap::new();

    for event in &bundle.events.properties {
        if event.property_name.as_ref() != "AttachSocket" {
            continue;
        }
        let Some(owner_guid) = event.actor_guid else {
            continue;
        };
        let Some(child_guid) = event.decoded.int_packed else {
            continue;
        };
        if owner_guid == child_guid {
            continue;
        }
        hints.entry(child_guid).or_insert(owner_guid);
    }

    hints
}

fn build_team_aliases(bundle: &Bundle) -> HashMap<u32, u32> {
    let mut aliases: HashMap<u32, u32> = HashMap::new();
    let mut faction_primary: HashMap<String, u32> = HashMap::new();

    for team in &bundle.teams {
        let team_id = team.id;
        let faction_key = team
            .faction
            .as_ref()
            .map(|f| f.to_lowercase())
            .unwrap_or_default();
        if faction_key.is_empty() {
            aliases.entry(team_id).or_insert(team_id);
            continue;
        }

        let preferred = faction_primary.entry(faction_key).or_insert(team_id);
        // Prefer canonical replay IDs 1/2 when available for that faction.
        if matches!(team_id, 1 | 2) {
            *preferred = team_id;
        }
    }

    for team in &bundle.teams {
        let team_id = team.id;
        let canonical = team
            .faction
            .as_ref()
            .map(|f| f.to_lowercase())
            .and_then(|f| faction_primary.get(&f).copied())
            .unwrap_or(team_id);
        aliases.insert(team_id, canonical);
    }

    aliases
}

fn build_owner_team_hints(bundle: &Bundle) -> HashMap<u32, u32> {
    let mut hints = HashMap::new();

    for event in &bundle.events.properties {
        if event.property_name.as_ref() != "Team" {
            continue;
        }
        let Some(actor_guid) = event.actor_guid else {
            continue;
        };
        let value = event
            .decoded
            .int_packed
            .or_else(|| event.decoded.int32.map(|v| v as u32));
        let Some(team_id) = value else {
            continue;
        };
        hints.insert(actor_guid, team_id);
    }

    hints
}

fn build_owner_player_state_windows(bundle: &Bundle) -> HashMap<u32, Vec<(u64, Option<u32>)>> {
    let mut windows: HashMap<u32, Vec<(u64, Option<u32>)>> = HashMap::new();

    for event in &bundle.events.properties {
        if event.property_name.as_ref() != "PlayerState" {
            continue;
        }
        let Some(actor_guid) = event.actor_guid else {
            continue;
        };
        let player_state = event
            .decoded
            .int_packed
            .or_else(|| event.decoded.int32.map(|v| v as u32))
            .and_then(|value| (value > 0).then_some(value));

        windows
            .entry(actor_guid)
            .or_default()
            .push((event.t_ms, player_state));
    }

    for entries in windows.values_mut() {
        entries.sort_by_key(|(t_ms, _)| *t_ms);
    }

    windows
}

fn build_player_name_by_state(bundle: &Bundle) -> HashMap<u32, String> {
    bundle
        .players
        .iter()
        .filter_map(|player| {
            player
                .name
                .as_ref()
                .map(|name| (player.player_state_guid, name.to_lowercase()))
        })
        .collect()
}

fn build_player_tracks_by_name(bundle: &Bundle) -> HashMap<String, Vec<TrackSample3>> {
    let mut tracks = HashMap::new();
    for track in &bundle.tracks.players {
        let samples = track.samples.clone();
        tracks.insert(track.key.to_lowercase(), samples.clone());

        if let Some(player_state_guid) = track.player_state_guid {
            if let Some(player) = bundle
                .players
                .iter()
                .find(|p| p.player_state_guid == player_state_guid)
            {
                if let Some(name) = player.name.as_ref() {
                    tracks.insert(name.to_lowercase(), samples.clone());
                }
                if let Some(eos_id) = player.eos_id.as_ref() {
                    tracks.insert(eos_id.to_lowercase(), samples.clone());
                }
            }
        }
    }
    tracks
}

fn infer_vehicle_samples_from_owner_player_state(
    owner_guid: u32,
    owner_player_states: &HashMap<u32, Vec<(u64, Option<u32>)>>,
    player_name_by_state: &HashMap<u32, String>,
    player_tracks_by_name: &HashMap<String, Vec<TrackSample3>>,
) -> Vec<TrackSample3> {
    let Some(events) = owner_player_states.get(&owner_guid) else {
        return Vec::new();
    };

    let mut samples = Vec::new();

    for (index, (start_ms, player_state)) in events.iter().enumerate() {
        let Some(player_state_guid) = player_state else {
            continue;
        };
        let Some(player_name) = player_name_by_state.get(player_state_guid) else {
            continue;
        };
        let Some(player_track) = player_tracks_by_name.get(player_name) else {
            continue;
        };

        let end_ms = events
            .get(index + 1)
            .map(|(next_t_ms, _)| *next_t_ms)
            .unwrap_or(u64::MAX);

        for sample in player_track {
            if sample.t_ms < *start_ms || sample.t_ms >= end_ms {
                continue;
            }
            samples.push(sample.clone());
        }
    }

    samples.sort_by_key(|sample| sample.t_ms);
    samples.dedup_by_key(|sample| sample.t_ms);
    samples
}

fn build_capture_zones(zones: &[CaptureZone]) -> Vec<TimelineCaptureZone> {
    zones
        .iter()
        .map(|zone| {
            let events: Vec<TimelineCaptureEvent> = zone
                .events
                .iter()
                .filter(|e| e.event_type == "owning_team")
                .map(|e| TimelineCaptureEvent {
                    t: e.t_ms,
                    owner: e.value_int.map(|v| normalize_team_id(v as u32)),
                })
                .collect();

            TimelineCaptureZone {
                name: zone.display_name.clone().or(zone.name.clone()),
                x: zone.x,
                y: zone.y,
                events,
            }
        })
        .collect()
}

fn build_deployables(bundle: &Bundle) -> Vec<TimelineDeployable> {
    // Build maps of actor_guid -> events from property events
    let mut health_events_by_actor: std::collections::HashMap<u32, Vec<TimelineHealthEvent>> =
        std::collections::HashMap::new();
    let mut ammo_events_by_actor: std::collections::HashMap<u32, Vec<TimelineAmmoEvent>> =
        std::collections::HashMap::new();
    let mut construction_events_by_actor: std::collections::HashMap<u32, Vec<TimelineConstructionEvent>> =
        std::collections::HashMap::new();

    for prop in &bundle.events.properties {
        if let Some(actor_guid) = prop.actor_guid {
            match prop.property_name.as_ref() {
                "Health" => {
                    if let Some(health) = prop.decoded.float32 {
                        health_events_by_actor
                            .entry(actor_guid)
                            .or_default()
                            .push(TimelineHealthEvent {
                                t: prop.t_ms,
                                health: health as f64,
                            });
                    }
                }
                "Ammo" => {
                    if let Some(ammo) = prop.decoded.float32 {
                        ammo_events_by_actor
                            .entry(actor_guid)
                            .or_default()
                            .push(TimelineAmmoEvent {
                                t: prop.t_ms,
                                ammo: ammo as f64,
                            });
                    }
                }
                "ConstructionPoints" => {
                    if let Some(points) = prop.decoded.float32 {
                        construction_events_by_actor
                            .entry(actor_guid)
                            .or_default()
                            .push(TimelineConstructionEvent {
                                t: prop.t_ms,
                                points: points as f64,
                            });
                    }
                }
                _ => {}
            }
        }
    }

    bundle
        .actors
        .deployables
        .iter()
        .map(|actor| {
            let class = actor
                .class_name
                .as_ref()
                .map(|c| extract_deployable_class(c))
                .unwrap_or_else(|| "Unknown".to_string());

            // Get health events for this deployable
            let mut health_events = health_events_by_actor
                .remove(&actor.actor_guid)
                .unwrap_or_default();

            // Sort by time and deduplicate consecutive same-health values
            health_events.sort_by_key(|e| e.t);
            let health_events: Vec<TimelineHealthEvent> = health_events
                .into_iter()
                .fold(Vec::new(), |mut acc, evt| {
                    if acc.last().map(|last| (last.health - evt.health).abs() > 0.01).unwrap_or(true) {
                        acc.push(evt);
                    }
                    acc
                });

            // Get ammo events for this deployable
            let mut ammo_events = ammo_events_by_actor
                .remove(&actor.actor_guid)
                .unwrap_or_default();

            // Sort by time and deduplicate consecutive same-ammo values
            ammo_events.sort_by_key(|e| e.t);
            let ammo_events: Vec<TimelineAmmoEvent> = ammo_events
                .into_iter()
                .fold(Vec::new(), |mut acc, evt| {
                    if acc.last().map(|last| (last.ammo - evt.ammo).abs() > 0.01).unwrap_or(true) {
                        acc.push(evt);
                    }
                    acc
                });

            // Get construction events for this deployable
            let mut construction_events = construction_events_by_actor
                .remove(&actor.actor_guid)
                .unwrap_or_default();

            // Sort by time and deduplicate consecutive same-points values
            construction_events.sort_by_key(|e| e.t);
            let construction_events: Vec<TimelineConstructionEvent> = construction_events
                .into_iter()
                .fold(Vec::new(), |mut acc, evt| {
                    if acc.last().map(|last| (last.points - evt.points).abs() > 0.01).unwrap_or(true) {
                        acc.push(evt);
                    }
                    acc
                });

            TimelineDeployable {
                class,
                team_id: actor.team.map(|t| normalize_team_id(t as u32)),
                x: actor.initial_location.map(|l| l.x).unwrap_or(0.0),
                y: actor.initial_location.map(|l| l.y).unwrap_or(0.0),
                z: actor.initial_location.map(|l| l.z).unwrap_or(0.0),
                placed_at: actor.open_time_ms,
                destroyed_at: actor.close_time_ms,
                health: actor.health,
                health_events,
                ammo_events,
                construction_events,
            }
        })
        .collect()
}

fn build_events(bundle: &Bundle, eos_to_player: &HashMap<String, &Player>) -> TimelineEvents {
    TimelineEvents {
        kills: build_kills(&bundle.events.kills, eos_to_player),
        spawns: build_spawns(&bundle.players),
    }
}

fn build_kills(kills: &[KillEvent], eos_to_player: &HashMap<String, &Player>) -> Vec<TimelineKillEvent> {
    kills
        .iter()
        .map(|kill| {
            // Try to find EOS IDs for victim and killer
            let victim_eos = kill.victim_name.as_ref().and_then(|name| {
                eos_to_player
                    .iter()
                    .find(|(_, p)| p.name.as_ref() == Some(name))
                    .map(|(eos, _)| eos.clone())
            });

            let killer_eos = kill.killer_name.as_ref().and_then(|name| {
                eos_to_player
                    .iter()
                    .find(|(_, p)| p.name.as_ref() == Some(name))
                    .map(|(eos, _)| eos.clone())
            });

            TimelineKillEvent {
                t: kill.t_ms,
                victim: victim_eos.or(kill.victim_name.clone()),
                killer: killer_eos.or(kill.killer_name.clone()),
                weapon: None, // Not available in current Bundle
                is_incap: kill.was_incap,
            }
        })
        .collect()
}

fn build_spawns(players: &[Player]) -> Vec<TimelineSpawnEvent> {
    let mut spawns = Vec::new();

    for player in players {
        let eos_id = match &player.eos_id {
            Some(id) => id.to_lowercase(),
            None => continue,
        };

        // Generate spawn events from visibility windows
        for window in &player.visibility_windows {
            spawns.push(TimelineSpawnEvent {
                t: window.start_ms,
                player: eos_id.clone(),
                kit: player.current_role_name.clone().or(player.deploy_role_name.clone()),
            });
        }
    }

    // Sort by time
    spawns.sort_by_key(|s| s.t);
    spawns
}

// ============================================================================
// Helper functions
// ============================================================================

/// Normalize team ID to 1 or 2 (never 0).
fn normalize_team_id(id: u32) -> u32 {
    match id {
        0 => 1,
        1 => 2,
        _ => id,
    }
}

/// Downsample track to target interval.
fn downsample_track(samples: &[TrackSample3], interval_ms: u64) -> Vec<TimelinePositionSample> {
    if samples.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut last_t: Option<u64> = None;

    for sample in samples {
        let should_include = match last_t {
            None => true,
            Some(lt) => sample.t_ms >= lt + interval_ms,
        };

        if should_include {
            result.push(TimelinePositionSample {
                t: sample.t_ms,
                x: round2(sample.x),
                y: round2(sample.y),
                z: round2(sample.z),
                yaw: sample.yaw.map(round2),
            });
            last_t = Some(sample.t_ms);
        }
    }

    result
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Extract short class name from full path.
/// "BP_BTR82A_RUS_C" -> "BTR82A"
fn extract_vehicle_class(full_name: &str) -> String {
    let name = full_name
        .trim_start_matches("BP_")
        .trim_end_matches("_C");
    
    // Remove faction suffixes
    for suffix in ["_RUS", "_USA", "_GB", "_CAF", "_AUS", "_TLF", "_MEA", "_MIL", "_INS", "_PLA", "_PLANMC", "_USMC"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    
    name.to_string()
}

/// Extract deployable class name.
fn extract_deployable_class(full_name: &str) -> String {
    let name = full_name
        .trim_start_matches("BP_")
        .trim_end_matches("_C");
    
    // Common patterns
    if name.contains("FOBRadio") || name.contains("FobRadio") {
        return "FOBRadio".to_string();
    }
    if name.contains("HAB") || name.contains("Hab") {
        return "HAB".to_string();
    }
    if name.contains("RallyPoint") || name.contains("Rallypoint") {
        return "RallyPoint".to_string();
    }
    if name.contains("AmmoCrate") || name.contains("Ammocrate") {
        return "AmmoCrate".to_string();
    }
    
    name.to_string()
}

// ============================================================================
// File I/O
// ============================================================================

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Write a timeline to a `.sqrt.json` file.
pub fn write_timeline(timeline: &Timeline, path: impl AsRef<Path>) -> crate::Result<()> {
    let path = path.as_ref();
    let file = File::create(path).map_err(|e| crate::Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, timeline)?;
    writer.flush().map_err(|e| crate::Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}
