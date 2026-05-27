//! Ticket proof output format for Squad match analysis.
//!
//! Produces a JSON file containing ticket-affecting events:
//! - Player deaths (from log)
//! - Vehicle destructions (from log) 
//! - Flag captures (from replay)
//! - FOB radio bleeds (from replay)

use crate::bundle::{Bundle, Team};
use crate::log_parser::LogMatch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Ticket proof format version
pub const TICKETPROOF_VERSION: u32 = 1;

// ============================================================================
// Data structures
// ============================================================================

/// Root ticket proof structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketProof {
    pub version: u32,
    pub meta: TicketMeta,
    pub teams: Vec<TicketTeam>,
    pub events: Vec<TicketEvent>,
    pub statistics: TicketStatistics,
    pub timeline: Vec<TicketTimelinePoint>,
    #[serde(rename = "final")]
    pub final_tickets: FinalTickets,
}

/// Final ticket counts at end of match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalTickets {
    pub team1: i32,
    pub team2: i32,
}

/// Match metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketMeta {
    pub map: Option<String>,
    pub layer: Option<String>,
    pub mode: Option<String>,
    pub duration_ms: u64,
    pub start_tickets: StartTickets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartTickets {
    pub team1: i32,
    pub team2: i32,
}

/// Team info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketTeam {
    pub id: u32,
    pub faction: Option<String>,
    pub name: Option<String>,
}

/// A ticket-affecting event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketEvent {
    pub t_ms: u64,
    pub kind: String,
    pub team: u32,
    pub delta: i32,
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Per-team statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketStatistics {
    pub team1: TeamStats,
    pub team2: TeamStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamStats {
    pub infantry_deaths: u32,
    pub vehicle_deaths: HashMap<String, u32>,
    pub captures: u32,
    pub radios_placed: u32,
    pub radios_bled: u32,
}

/// Timeline point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketTimelinePoint {
    pub t_ms: u64,
    pub team1: i32,
    pub team2: i32,
}

// ============================================================================
// Vehicle classification
// ============================================================================

#[derive(Debug, Clone)]
struct VehicleInfo {
    tickets: i32,
    category: &'static str,
    label: &'static str,
}

fn classify_vehicle(name: &str) -> Option<VehicleInfo> {
    let upper = name.to_uppercase();
    
    // MBTs - 15 tickets
    if upper.contains("M1A2") || upper.contains("T72") || upper.contains("T64") 
        || upper.contains("T90") || upper.contains("LEOPARD") || upper.contains("CHALLENGER")
        || upper.contains("MERKAVA") || upper.contains("M60T") || upper.contains("ZTZ99")
        || upper.contains("TYPE99") {
        return Some(VehicleInfo { tickets: 15, category: "mbt", label: "Main Battle Tank" });
    }
    
    // IFVs - 10 tickets
    if upper.contains("BTR4") || upper.contains("BMP") || upper.contains("BRADLEY")
        || upper.contains("MARDER") || upper.contains("WARRIOR") || upper.contains("CV90")
        || upper.contains("LAV6") || upper.contains("LAV_6") || upper.contains("ASLAV") {
        return Some(VehicleInfo { tickets: 10, category: "ifv", label: "Infantry Fighting Vehicle" });
    }
    
    // APCs - 8 tickets
    if upper.contains("BTR80") || upper.contains("BTR82") || upper.contains("STRYKER")
        || upper.contains("MTLB") || upper.contains("BULLDOG") || upper.contains("TLAV")
        || upper.contains("M113") {
        return Some(VehicleInfo { tickets: 8, category: "apc", label: "Armored Personnel Carrier" });
    }
    
    // Helicopters - 10 tickets (transport), 15 tickets (attack)
    if upper.contains("UH60") || upper.contains("MI8") || upper.contains("CH146")
        || upper.contains("MRH90") || upper.contains("SA330") || upper.contains("UH1Y") {
        return Some(VehicleInfo { tickets: 10, category: "transport_helo", label: "Transport Helicopter" });
    }
    if upper.contains("AH64") || upper.contains("MI24") || upper.contains("KA52")
        || upper.contains("AH1Z") {
        return Some(VehicleInfo { tickets: 15, category: "attack_helo", label: "Attack Helicopter" });
    }
    
    // Logistics - 5 tickets
    if upper.contains("LOGI") || upper.contains("LOGISTICS") {
        return Some(VehicleInfo { tickets: 5, category: "logi", label: "Logistics Truck" });
    }
    
    // Transport trucks - 3 tickets
    if upper.contains("UTIL") || upper.contains("TRANSPORT") || upper.contains("URAL")
        || upper.contains("M939") || upper.contains("KAMAZ") || upper.contains("MTVR") {
        return Some(VehicleInfo { tickets: 3, category: "truck", label: "Transport Truck" });
    }
    
    // Technicals - 3 tickets
    if upper.contains("TECHNICAL") || upper.contains("SIMIR") || upper.contains("SAFIR") {
        return Some(VehicleInfo { tickets: 3, category: "technical", label: "Technical" });
    }
    
    // MRAPs - 5 tickets
    if upper.contains("MRAP") || upper.contains("MATV") || upper.contains("BRDM")
        || upper.contains("TAPV") || upper.contains("TIGR") || upper.contains("FENNEK") {
        return Some(VehicleInfo { tickets: 5, category: "mrap", label: "Light Armored Vehicle" });
    }
    
    // Default for unrecognized vehicles
    Some(VehicleInfo { tickets: 3, category: "unknown", label: "Vehicle" })
}

/// Determine team from vehicle class name
fn team_from_vehicle_name(name: &str) -> Option<u32> {
    let upper = name.to_uppercase();
    
    // Team 1 factions (typically NATO/BLUFOR)
    if upper.contains("USA") || upper.contains("USMC") || upper.contains("CAF")
        || upper.contains("BAF") || upper.contains("ADF") || upper.contains("WPMC")
        || upper.contains("WOODLAND") || upper.contains("C16") {
        return Some(1);
    }
    
    // Team 2 factions (typically OPFOR/independent)
    if upper.contains("AFU") || upper.contains("TLF") || upper.contains("RUS")
        || upper.contains("VDV") || upper.contains("MEA") || upper.contains("INS")
        || upper.contains("MIL") || upper.contains("PLA") || upper.contains("PLANMC") {
        return Some(2);
    }
    
    None
}

/// Clean vehicle class name for display
fn clean_vehicle_name(name: &str) -> String {
    name.trim_start_matches("BP_")
        .split("_C_")
        .next()
        .unwrap_or(name)
        .replace('_', " ")
}

// ============================================================================
// Builder
// ============================================================================

/// Options for building ticket proof
#[derive(Debug, Clone, Default)]
pub struct TicketProofOptions {
    /// Starting tickets for team 1 (default: determined by mode)
    pub start_team1: Option<i32>,
    /// Starting tickets for team 2 (default: determined by mode)
    pub start_team2: Option<i32>,
}

/// Build ticket proof from a parsed bundle
pub fn build_ticketproof(
    bundle: &Bundle,
    log_match: Option<&LogMatch>,
    options: &TicketProofOptions,
) -> TicketProof {
    let mode = extract_mode(bundle);
    let (default_t1, default_t2) = mode_start_tickets(&mode);
    
    let start_t1 = options.start_team1.unwrap_or(default_t1);
    let start_t2 = options.start_team2.unwrap_or(default_t2);
    
    let mut events = Vec::new();
    let mut stats = TicketStatistics {
        team1: TeamStats::default(),
        team2: TeamStats::default(),
    };
    
    // Build team lookup
    let team_map = build_team_map(&bundle.teams);
    let player_teams = build_player_team_lookup(bundle);
    
    // Collect infantry deaths from kill events
    for kill in &bundle.events.kills {
        if let Some(victim_name) = &kill.victim_name {
            if let Some(team) = player_teams.get(&victim_name.to_lowercase()) {
                events.push(TicketEvent {
                    t_ms: kill.t_ms,
                    kind: "infantry_death".to_string(),
                    team: *team,
                    delta: -1,
                    detail: Some(victim_name.clone()),
                    category: None,
                    icon: None,
                });
                
                if *team == 1 {
                    stats.team1.infantry_deaths += 1;
                } else {
                    stats.team2.infantry_deaths += 1;
                }
            }
        }
    }
    
    // Collect vehicle deaths from log events
    for veh_death in &bundle.events.vehicle_deaths {
        // Determine team from vehicle class name
        if let Some(info) = classify_vehicle(&veh_death.vehicle_class) {
            let team = team_from_vehicle_name(&veh_death.vehicle_class).unwrap_or(0);
            if team == 1 || team == 2 {
                let clean_name = clean_vehicle_name(&veh_death.vehicle_class);
                
                events.push(TicketEvent {
                    t_ms: veh_death.t_ms,
                    kind: "vehicle_death".to_string(),
                    team,
                    delta: -info.tickets,
                    detail: Some(clean_name.clone()),
                    category: Some(info.category.to_string()),
                    icon: None,
                });
                
                let category = info.category.to_string();
                if team == 1 {
                    *stats.team1.vehicle_deaths.entry(category).or_insert(0) += 1;
                } else {
                    *stats.team2.vehicle_deaths.entry(category).or_insert(0) += 1;
                }
            }
        }
    }
    
    // Collect capture zone events
    for zone in &bundle.events.capture_zones {
        let zone_name = zone.name.as_deref().unwrap_or("Unknown");
        
        // Skip main bases
        if zone_name.to_lowercase().contains("main") {
            continue;
        }
        
        let mut prev_owner: Option<u32> = None;
        for event in &zone.events {
            // Only look at ownership changes
            if event.event_type != "owning_team" {
                continue;
            }
            
            let raw_owner = event.value_int.unwrap_or(0) as u32;
            let owner = normalize_team_id(raw_owner, &team_map);
            if owner != 1 && owner != 2 {
                continue;
            }
            
            let (delta, kind) = if prev_owner.is_none() {
                (20, "capture_first")
            } else if prev_owner != Some(owner) {
                (50, "capture_enemy")
            } else {
                prev_owner = Some(owner);
                continue;
            };
            
            events.push(TicketEvent {
                t_ms: event.t_ms,
                kind: kind.to_string(),
                team: owner,
                delta,
                detail: Some(zone_name.to_string()),
                category: None,
                icon: Some("flag.svg".to_string()),
            });
            
            if owner == 1 {
                stats.team1.captures += 1;
            } else {
                stats.team2.captures += 1;
            }
            
            prev_owner = Some(owner);
        }
    }
    
    // Collect radio bleed events from deployables
    for dep in &bundle.actors.deployables {
        let class = dep.class_name.as_deref().unwrap_or("");
        if !class.to_lowercase().contains("radio") {
            continue;
        }
        
        let team = dep.team.map(|t| normalize_team_id(t as u32, &team_map)).unwrap_or(0);
        if team != 1 && team != 2 {
            continue;
        }
        
        // Count placement
        if team == 1 {
            stats.team1.radios_placed += 1;
        } else {
            stats.team2.radios_placed += 1;
        }
        
        // Check for bleed (destroyed with low health)
        if let Some(close_time) = dep.close_time_ms {
            // Health below critical threshold indicates bleed
            if let Some(health) = dep.health {
                if health <= 24.0 {
                    events.push(TicketEvent {
                        t_ms: close_time,
                        kind: "radio_bleed".to_string(),
                        team,
                        delta: -20,
                        detail: Some("FOBRadio".to_string()),
                        category: None,
                        icon: Some("deployable_fob.svg".to_string()),
                    });
                    
                    if team == 1 {
                        stats.team1.radios_bled += 1;
                    } else {
                        stats.team2.radios_bled += 1;
                    }
                }
            }
        }
    }
    
    // Sort events by time
    events.sort_by_key(|e| e.t_ms);
    
    // Build timeline
    let mut t1 = start_t1;
    let mut t2 = start_t2;
    let mut timeline = vec![TicketTimelinePoint { t_ms: 0, team1: t1, team2: t2 }];
    
    for event in &events {
        if event.team == 1 {
            t1 += event.delta;
        } else {
            t2 += event.delta;
        }
        timeline.push(TicketTimelinePoint {
            t_ms: event.t_ms,
            team1: t1,
            team2: t2,
        });
    }
    
    // Add final point at match end
    if bundle.replay.duration_ms > 0 {
        timeline.push(TicketTimelinePoint {
            t_ms: bundle.replay.duration_ms,
            team1: t1,
            team2: t2,
        });
    }
    
    TicketProof {
        version: TICKETPROOF_VERSION,
        meta: TicketMeta {
            map: bundle.replay.map_name.clone(),
            layer: bundle.replay.layer_name.clone(),
            mode: Some(mode),
            duration_ms: bundle.replay.duration_ms,
            start_tickets: StartTickets {
                team1: start_t1,
                team2: start_t2,
            },
        },
        teams: bundle.teams.iter().map(|t| TicketTeam {
            id: normalize_team_id(t.id, &team_map),
            faction: t.faction.clone(),
            name: t.name.clone(),
        }).collect(),
        events,
        statistics: stats,
        final_tickets: FinalTickets { team1: t1, team2: t2 },
        timeline,
    }
}

fn extract_mode(bundle: &Bundle) -> String {
    bundle.game_state.game_mode.clone()
        .or_else(|| {
            bundle.replay.layer_name.as_ref().and_then(|layer| {
                let parts: Vec<&str> = layer.split('_').collect();
                parts.get(1).map(|s| s.to_string())
            })
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn mode_start_tickets(mode: &str) -> (i32, i32) {
    let lower = mode.to_lowercase();
    if lower.contains("invasion") {
        (800, 200)
    } else {
        (250, 250)
    }
}

fn build_team_map(teams: &[Team]) -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    
    // The bundle uses 0-based team IDs, normalize to 1-based
    // Team 0 -> 1 (first team), Team 1 -> 2 (second team)
    map.insert(0, 1);
    map.insert(1, 2);
    
    // Also keep 1 and 2 mapped to themselves for cases where
    // the bundle already uses 1-based IDs
    map.insert(2, 2);
    
    // Map higher team IDs (like 8276) to their canonical team based on faction
    if teams.len() >= 2 {
        let faction_0 = teams.get(0).and_then(|t| t.faction.as_ref());
        let faction_1 = teams.get(1).and_then(|t| t.faction.as_ref());
        
        for team in teams.iter().skip(2) {
            if team.id > 2 {
                if team.faction.as_ref() == faction_0 {
                    map.insert(team.id, 1);
                } else if team.faction.as_ref() == faction_1 {
                    map.insert(team.id, 2);
                }
            }
        }
    }
    
    map
}

fn normalize_team_id(raw: u32, team_map: &HashMap<u32, u32>) -> u32 {
    *team_map.get(&raw).unwrap_or(&raw)
}

fn build_player_team_lookup(bundle: &Bundle) -> HashMap<String, u32> {
    let team_map = build_team_map(&bundle.teams);
    let mut lookup = HashMap::new();
    
    for player in &bundle.players {
        if let (Some(name), Some(team_id)) = (&player.name, player.team_id) {
            let canonical = normalize_team_id(team_id, &team_map);
            if canonical == 1 || canonical == 2 {
                lookup.insert(name.to_lowercase(), canonical);
            }
        }
    }
    
    lookup
}

/// Write ticket proof to a JSON file
pub fn write_ticketproof(tp: &TicketProof, path: impl AsRef<Path>) -> crate::Result<()> {
    let path = path.as_ref();
    let file = File::create(path).map_err(|e| crate::Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, tp)?;
    Ok(())
}
