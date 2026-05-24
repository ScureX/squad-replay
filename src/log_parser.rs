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

/// Extract map name and layer from path segments.
/// Path examples:
/// - /Game/Maps/Narva/Gameplay_Layers/Narva_RAAS_v1.Narva_RAAS_v1
/// - /Al_Basrah/Maps/Gameplay_Layers/AlBasrah_RAAS_v3.AlBasrah_RAAS_v3
/// - /Game/Maps/TransitionMap.TransitionMap
fn extract_map_and_layer(parts: &[&str]) -> (String, String) {
    // Find the layer name (last segment, strip .Extension)
    let layer = parts.last()
        .map(|s| s.split('.').next().unwrap_or(s).to_string())
        .unwrap_or_default();
    
    // Find map name by looking for the meaningful segment
    // Skip: "Game", "Maps", "Gameplay_Layers", "Coop", and the layer file
    let map = parts.iter()
        .filter(|&&s| {
            s != "Game" && 
            s != "Maps" && 
            s != "Gameplay_Layers" && 
            s != "Coop" &&
            s != "FoundersPack" &&
            s != "FreeMissions" &&
            !s.contains('.')  // Skip layer filename
        })
        .next()
        .map(|s| s.to_string())
        .unwrap_or_else(|| layer.clone());
    
    (map, layer)
}

/// Parse a Squad game log file
pub fn parse_log_file(path: &Path) -> Result<Vec<LogMatch>, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    // Compile regexes - capture the full path and extract map name later
    let re_new_game = Regex::new(
        r"^\[([0-9.:-]+)\]\[([ 0-9]*)\]LogWorld: Bringing World (/[^\s]+) up for play"
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
            
            // Extract map and layer from full path like:
            // /Game/Maps/Narva/Gameplay_Layers/Narva_RAAS_v1.Narva_RAAS_v1
            // /Al_Basrah/Maps/Gameplay_Layers/AlBasrah_RAAS_v3.AlBasrah_RAAS_v3
            let full_path = &caps[3];
            let parts: Vec<&str> = full_path.split('/').filter(|s| !s.is_empty()).collect();
            
            // Find map name: look for segment before "Gameplay_Layers" or "Maps"
            // or extract from the layer filename
            let (map, layer) = extract_map_and_layer(&parts);
            
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

/// Find the best matching log match for a replay by comparing times.
/// The replay file's modification time should be close to when the match ended.
/// tz_offset_hours adjusts log timestamps (e.g., 2 if server is UTC and local is UTC+2).
pub fn find_matching_log<'a>(
    log_matches: &'a [LogMatch],
    replay_duration_ms: u64,
    replay_end_time: Option<chrono::NaiveDateTime>,
    tz_offset_hours: i32,
) -> Option<&'a LogMatch> {
    println!("[log] Searching {} matches in log", log_matches.len());
    
    if let Some(replay_end) = replay_end_time {
        println!("[log] Replay file time: {}", replay_end.format("%Y-%m-%d %H:%M:%S"));
    }
    if tz_offset_hours != 0 {
        println!("[log] Applying timezone offset: +{}h to log times", tz_offset_hours);
    }
    
    let tz_offset = chrono::Duration::hours(tz_offset_hours as i64);
    
    let mut best_match: Option<&LogMatch> = None;
    let mut best_time_diff: i64 = i64::MAX;
    
    for log_match in log_matches {
        // Get the log match end time, adjusted for timezone
        let log_end = match log_match.end_time {
            Some(t) => t + tz_offset,
            None => continue, // Skip matches without end time
        };
        
        // If we have replay end time, match by time proximity
        if let Some(replay_end) = replay_end_time {
            // The replay file modification time should be close to match end time
            // Allow some tolerance (replay might be written a few seconds/minutes after match end)
            let time_diff = (replay_end - log_end).num_seconds().abs();
            
            // Must be within 30 minutes of each other
            if time_diff > 30 * 60 {
                continue;
            }
            
            // Check duration: replay can be shorter (user exited early) but not much longer
            let log_duration = log_match.duration_ms();
            let replay_longer_by = replay_duration_ms as i64 - log_duration as i64;
            
            // Reject if replay is significantly longer than the match (more than 5 min)
            // But allow replay to be shorter (user ended early)
            if replay_longer_by > 5 * 60 * 1000 {
                continue;
            }
            
            if time_diff < best_time_diff {
                best_time_diff = time_diff;
                best_match = Some(log_match);
            }
        } else {
            // Fallback: match by duration only (less reliable)
            let log_duration = log_match.duration_ms();
            let duration_diff = (log_duration as i64 - replay_duration_ms as i64).abs();
            let duration_tolerance = (replay_duration_ms as i64) / 10; // 10%
            
            if duration_diff < duration_tolerance && duration_diff < best_time_diff {
                best_time_diff = duration_diff;
                best_match = Some(log_match);
            }
        }
    }
    
    if let Some(m) = best_match {
        let end_str = m.end_time.map(|t| (t + tz_offset).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default();
        println!("[log] Match found: {} - {} (ended {}, {} events)", 
            m.map, m.layer, end_str, m.events.len());
    } else {
        println!("[log] No matching log found. Available matches:");
        for m in log_matches {
            let end_str = m.end_time.map(|t| (t + tz_offset).format("%H:%M:%S").to_string()).unwrap_or("ongoing".to_string());
            let dur_min = m.duration_ms() / 60000;
            println!("[log]   {} - {} (ended {}, {}min)", m.map, m.layer, end_str, dur_min);
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
    let mut updated_count = 0;
    for player in players.iter_mut() {
        let eos_id = match &player.eos_id {
            Some(id) => id.to_lowercase(),
            None => continue,
        };
        
        if let Some(state) = player_states.get(&eos_id) {
            updated_count += 1;
            
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
    
    println!("[log] Merged log data into {}/{} players ({} events in log)", 
        updated_count, players.len(), log_match.events.len());
}
