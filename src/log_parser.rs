//! Parser for Squad game log files (SquadGame.log)
//!
//! Extracts events like player connect/disconnect, spawn/death, etc.

use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::bundle::{Player, VisibilityWindow};

/// A parsed log event
#[derive(Debug, Clone)]
pub struct LogEvent {
    pub t_ms: u64,
    pub event_type: LogEventType,
}

#[derive(Debug, Clone)]
pub enum LogEventType {
    NewGame {
        map: String,
        layer: String,
    },
    PlayerConnected {
        controller: String,
        eos_id: Option<String>,
        steam_id: Option<String>,
    },
    PlayerDisconnected {
        controller: String,
        eos_id: Option<String>,
    },
    PlayerPossess {
        controller: String,
        eos_id: Option<String>,
        pawn: String,
    },
    PlayerUnpossess {
        controller: String,
        eos_id: Option<String>,
    },
    PlayerWounded {
        victim: String,
        attacker: String,
        weapon: String,
    },
    PlayerDied {
        victim: String,
        attacker: String,
        weapon: String,
    },
    RoundEnd,
}

/// A match/round from the log
#[derive(Debug, Clone)]
pub struct LogMatch {
    pub map: String,
    pub layer: String,
    pub start_time: chrono::NaiveDateTime,
    pub end_time: Option<chrono::NaiveDateTime>,
    pub events: Vec<LogEvent>,
}

impl LogMatch {
    pub fn duration_ms(&self) -> u64 {
        if let Some(end) = self.end_time {
            ((end - self.start_time).num_milliseconds().max(0)) as u64
        } else {
            0
        }
    }
}

/// Player state tracking from log events
#[derive(Debug, Default)]
pub struct PlayerLogState {
    pub eos_id: String,
    pub steam_id: Option<String>,
    pub name: Option<String>,
    pub connect_time_ms: Option<u64>,
    pub disconnect_time_ms: Option<u64>,
    pub spawn_events: Vec<SpawnEvent>,
    pub is_spawned: bool,
    pub current_pawn: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SpawnEvent {
    pub t_ms: u64,
    pub event_type: SpawnEventType,
    pub pawn: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpawnEventType {
    Spawn,
    Despawn,
    EnterVehicle,
    Wounded,
}

/// Parse a Squad log timestamp: 2026.05.24-11.46.32:407
fn parse_timestamp(ts: &str) -> Option<chrono::NaiveDateTime> {
    // Format: YYYY.MM.DD-HH.MM.SS:mmm
    let parts: Vec<&str> = ts.split(&['.', '-', ':'][..]).collect();
    if parts.len() < 7 {
        return None;
    }
    
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let hour: u32 = parts[3].parse().ok()?;
    let min: u32 = parts[4].parse().ok()?;
    let sec: u32 = parts[5].parse().ok()?;
    let millis: u32 = parts[6].parse().ok()?;
    
    chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_milli_opt(hour, min, sec, millis)
}

/// Extract EOS ID from online IDs string
fn extract_eos_id(ids: &str) -> Option<String> {
    let re = Regex::new(r"EOS:\s*([a-f0-9]+)").ok()?;
    re.captures(ids).map(|c| c[1].to_lowercase())
}

/// Extract Steam ID from online IDs string  
fn extract_steam_id(ids: &str) -> Option<String> {
    let re = Regex::new(r"steam:\s*(\d+)").ok()?;
    re.captures(ids).map(|c| c[1].to_string())
}

/// Parse a Squad game log file
pub fn parse_log_file(path: &Path) -> Result<Vec<LogMatch>, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    // Compile regexes
    let re_new_game = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogWorld: Bringing World /([A-Za-z0-9]+)/(?:Maps/)?([A-Za-z0-9_-]+)/(?:.+/)?([A-Za-z0-9_-]+)"
    ).unwrap();
    
    let re_player_connected = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogSquad: PostLogin: NewPlayer: .*?BP_PlayerController.*?\.([^\s]+) \(IP: [\d.]+ \| Online IDs:([^)]+)\)"
    ).unwrap();
    
    let re_player_disconnected = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogNet: UChannel::Close: Sending CloseBunch\..+PC: (\w+PlayerController[^,]*),.*?UniqueId: RedpointEOS:([a-f0-9]+)"
    ).unwrap();
    
    let re_player_possess = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogSquadTrace: \[DedicatedServer\](?:ASQPlayerController::)?OnPossess\(\): PC=(.+) \(Online IDs:([^)]+)\) Pawn=([A-Za-z0-9_]+)_C"
    ).unwrap();
    
    let re_player_unpossess = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogSquadTrace: \[DedicatedServer\](?:ASQPlayerController::)?OnUnPossess\(\): PC=(.+) \(Online IDs:([^)]+)\)"
    ).unwrap();
    
    let re_player_wounded = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogSquadTrace: \[DedicatedServer\](?:ASQSoldier::)?Wound\(\): Player:(.+) KillingDamage=[-0-9.]+ from ([A-Za-z_0-9]+) \(Online IDs:[^)]+\| Controller ID: [\w\d]+\) caused by ([A-Za-z_0-9-]+)_C"
    ).unwrap();
    
    let re_player_died = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogSquadTrace: \[DedicatedServer\](?:ASQSoldier::)?Die\(\): Player:(.+) KillingDamage=[-0-9.]+ from ([A-Za-z_0-9]+) \(Online IDs:[^)]+\| Contoller ID: [\w\d]+\) caused by ([A-Za-z_0-9-]+)_C"
    ).unwrap();
    
    let re_round_end = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogGameState: Match State Changed from InProgress to WaitingPostMatch"
    ).unwrap();
    
    let mut matches: Vec<LogMatch> = Vec::new();
    let mut current_match: Option<LogMatch> = None;
    
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        
        // Check for new game
        if let Some(caps) = re_new_game.captures(&line) {
            let timestamp = match parse_timestamp(&caps[1]) {
                Some(t) => t,
                None => continue,
            };
            let map = caps[4].to_string();
            let layer = caps[5].to_string();
            
            // Skip transition maps
            if map.contains("Transition") {
                continue;
            }
            
            // End previous match
            if let Some(mut m) = current_match.take() {
                if m.end_time.is_none() {
                    m.end_time = Some(timestamp);
                }
                matches.push(m);
            }
            
            // Start new match
            current_match = Some(LogMatch {
                map,
                layer,
                start_time: timestamp,
                end_time: None,
                events: Vec::new(),
            });
            continue;
        }
        
        let current = match current_match.as_mut() {
            Some(m) => m,
            None => continue,
        };
        
        // Player connected
        if let Some(caps) = re_player_connected.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerConnected {
                        controller: caps[3].to_string(),
                        eos_id: extract_eos_id(&caps[4]),
                        steam_id: extract_steam_id(&caps[4]),
                    },
                });
            }
            continue;
        }
        
        // Player disconnected
        if let Some(caps) = re_player_disconnected.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerDisconnected {
                        controller: caps[3].to_string(),
                        eos_id: Some(caps[4].to_lowercase()),
                    },
                });
            }
            continue;
        }
        
        // Player possess
        if let Some(caps) = re_player_possess.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerPossess {
                        controller: caps[3].to_string(),
                        eos_id: extract_eos_id(&caps[4]),
                        pawn: caps[5].to_string(),
                    },
                });
            }
            continue;
        }
        
        // Player unpossess
        if let Some(caps) = re_player_unpossess.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerUnpossess {
                        controller: caps[3].to_string(),
                        eos_id: extract_eos_id(&caps[4]),
                    },
                });
            }
            continue;
        }
        
        // Player wounded
        if let Some(caps) = re_player_wounded.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerWounded {
                        victim: caps[3].to_string(),
                        attacker: caps[4].to_string(),
                        weapon: caps[5].to_string(),
                    },
                });
            }
            continue;
        }
        
        // Player died
        if let Some(caps) = re_player_died.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::PlayerDied {
                        victim: caps[3].to_string(),
                        attacker: caps[4].to_string(),
                        weapon: caps[5].to_string(),
                    },
                });
            }
            continue;
        }
        
        // Round end
        if let Some(caps) = re_round_end.captures(&line) {
            if let Some(timestamp) = parse_timestamp(&caps[1]) {
                current.end_time = Some(timestamp);
                let t_ms = ((timestamp - current.start_time).num_milliseconds().max(0)) as u64;
                current.events.push(LogEvent {
                    t_ms,
                    event_type: LogEventType::RoundEnd,
                });
            }
            continue;
        }
    }
    
    // Don't forget last match
    if let Some(m) = current_match {
        matches.push(m);
    }
    
    Ok(matches)
}

/// Find the best matching log match for a replay
pub fn find_matching_log<'a>(
    log_matches: &'a [LogMatch],
    replay_map: &str,
    replay_duration_ms: u64,
) -> Option<&'a LogMatch> {
    let mut best_match: Option<&LogMatch> = None;
    let mut best_score = 0i32;
    
    for log_match in log_matches {
        let mut score = 0i32;
        
        // Check map name match
        let log_map = log_match.map.to_lowercase();
        let replay_map_lower = replay_map.to_lowercase();
        
        if log_map.contains(&replay_map_lower) || replay_map_lower.contains(&log_map) {
            score += 10;
        }
        
        // Check duration similarity
        let log_duration = log_match.duration_ms();
        let diff = (log_duration as i64 - replay_duration_ms as i64).unsigned_abs();
        let tolerance = replay_duration_ms / 10; // 10%
        
        if diff < tolerance {
            score += 5;
        } else if diff < tolerance * 2 {
            score += 2;
        }
        
        // Prefer matches with more events
        score += (log_match.events.len() / 100).min(3) as i32;
        
        if score > best_score {
            best_score = score;
            best_match = Some(log_match);
        }
    }
    
    best_match
}

/// Merge log events into player data
pub fn merge_log_into_players(
    players: &mut [Player],
    log_match: &LogMatch,
    duration_ms: u64,
) {
    // Build player state from log events
    let mut player_states: HashMap<String, PlayerLogState> = HashMap::new();
    
    for event in &log_match.events {
        match &event.event_type {
            LogEventType::PlayerConnected { eos_id, steam_id, controller } => {
                if let Some(eos) = eos_id {
                    let state = player_states.entry(eos.clone()).or_default();
                    state.eos_id = eos.clone();
                    state.steam_id = steam_id.clone();
                    state.name = Some(controller.clone());
                    state.connect_time_ms = Some(event.t_ms);
                }
            }
            LogEventType::PlayerDisconnected { eos_id, .. } => {
                if let Some(eos) = eos_id {
                    if let Some(state) = player_states.get_mut(eos) {
                        state.disconnect_time_ms = Some(event.t_ms);
                        if state.is_spawned {
                            state.spawn_events.push(SpawnEvent {
                                t_ms: event.t_ms,
                                event_type: SpawnEventType::Despawn,
                                pawn: None,
                            });
                            state.is_spawned = false;
                        }
                    }
                }
            }
            LogEventType::PlayerPossess { eos_id, pawn, controller } => {
                if let Some(eos) = eos_id {
                    let state = player_states.entry(eos.clone()).or_default();
                    state.eos_id = eos.clone();
                    if state.name.is_none() {
                        state.name = Some(controller.clone());
                    }
                    
                    let is_soldier = pawn.contains("Soldier");
                    let is_vehicle = pawn.contains("Vehicle") || 
                        ["Kamaz", "BTR", "BMP", "Tank", "Truck", "Tigr", "LPPV", "FV", "Helicopter", "MI8", "SA330"]
                            .iter().any(|v| pawn.contains(v));
                    
                    if is_soldier && !state.is_spawned {
                        state.spawn_events.push(SpawnEvent {
                            t_ms: event.t_ms,
                            event_type: SpawnEventType::Spawn,
                            pawn: Some(pawn.clone()),
                        });
                        state.is_spawned = true;
                    } else if is_vehicle {
                        state.spawn_events.push(SpawnEvent {
                            t_ms: event.t_ms,
                            event_type: SpawnEventType::EnterVehicle,
                            pawn: Some(pawn.clone()),
                        });
                    }
                    state.current_pawn = Some(pawn.clone());
                }
            }
            LogEventType::PlayerUnpossess { eos_id, .. } => {
                if let Some(eos) = eos_id {
                    if let Some(state) = player_states.get_mut(eos) {
                        // Just mark unpossess, death will handle despawn
                        state.current_pawn = None;
                    }
                }
            }
            LogEventType::PlayerWounded { .. } => {
                // Could track wounded state if needed
            }
            LogEventType::PlayerDied { .. } => {
                // Death handling - player is no longer spawned
                // Note: we don't have the victim's EOS ID directly here
            }
            _ => {}
        }
    }
    
    // Apply log data to players
    for player in players.iter_mut() {
        let eos_id = match &player.eos_id {
            Some(id) => id.to_lowercase(),
            None => continue,
        };
        
        if let Some(state) = player_states.get(&eos_id) {
            // Set connect/disconnect times
            player.connect_time_ms = state.connect_time_ms;
            player.disconnect_time_ms = state.disconnect_time_ms;
            
            // Compute visibility windows from spawn events
            let mut windows: Vec<VisibilityWindow> = Vec::new();
            let mut current_start: Option<u64> = None;
            
            for spawn in &state.spawn_events {
                match spawn.event_type {
                    SpawnEventType::Spawn => {
                        if current_start.is_none() {
                            current_start = Some(spawn.t_ms);
                        }
                    }
                    SpawnEventType::Despawn => {
                        if let Some(start) = current_start.take() {
                            windows.push(VisibilityWindow {
                                start_ms: start,
                                end_ms: spawn.t_ms,
                            });
                        }
                    }
                    _ => {}
                }
            }
            
            // Close any open window at end
            if let Some(start) = current_start {
                let end = state.disconnect_time_ms.unwrap_or(duration_ms);
                windows.push(VisibilityWindow {
                    start_ms: start,
                    end_ms: end,
                });
            }
            
            // Only override if we got windows from log (more accurate)
            if !windows.is_empty() {
                player.visibility_windows = windows;
            }
        }
    }
}
