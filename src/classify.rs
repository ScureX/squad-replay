/// Cached classification bitmask for an export group's class name.
///
/// The `is_*` helpers below do naive substring scans. They get called on
/// every property event, but only a few dozen unique class names exist per
/// replay, so we run them once per `ExportGroup` and cache the result here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassifyFlags(pub u8);

impl ClassifyFlags {
    pub const SOLDIER: u8 = 1 << 0;
    pub const VEHICLE: u8 = 1 << 1;
    pub const HELICOPTER: u8 = 1 << 2;
    pub const DEPLOYABLE_PRIMARY: u8 = 1 << 3;

    pub fn from_group_leaf(leaf: &str) -> Self {
        let mut bits = 0u8;
        if is_soldier_type(leaf) {
            bits |= Self::SOLDIER;
        }
        if is_helicopter_type(leaf) {
            bits |= Self::HELICOPTER;
        }
        if is_vehicle_type(leaf) {
            bits |= Self::VEHICLE;
        }
        if is_deployable_primary_type(leaf) {
            bits |= Self::DEPLOYABLE_PRIMARY;
        }
        Self(bits)
    }

    #[inline]
    pub fn is_soldier(self) -> bool {
        self.0 & Self::SOLDIER != 0
    }

    #[inline]
    pub fn is_vehicle(self) -> bool {
        self.0 & Self::VEHICLE != 0
    }

    #[inline]
    #[allow(dead_code)]
    pub fn is_helicopter(self) -> bool {
        self.0 & Self::HELICOPTER != 0
    }

    #[inline]
    pub fn is_deployable_primary(self) -> bool {
        self.0 & Self::DEPLOYABLE_PRIMARY != 0
    }
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    needle.is_empty()
        || haystack.len() >= needle.len()
            && haystack
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle))
}

fn starts_with_ignore_ascii_case(haystack: &str, prefix: &str) -> bool {
    let haystack = haystack.as_bytes();
    let prefix = prefix.as_bytes();
    haystack.len() >= prefix.len() && haystack[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub fn normalize_type(type_name: &str) -> Option<String> {
    if type_name.is_empty() {
        return None;
    }
    if let Some(idx) = type_name.rfind('.') {
        return Some(type_name[idx + 1..].to_string());
    }
    Some(type_name.to_string())
}

pub fn is_soldier_type(type_name: &str) -> bool {
    contains_ignore_ascii_case(type_name, "soldier")
}

pub fn is_helicopter_type(type_name: &str) -> bool {
    [
        "loach",
        "uh1",
        "uh-1",
        "uh60",
        "uh-60",
        "blackhawk",
        "black hawk",
        "mi8",
        "mi-8",
        "mi17",
        "mi-17",
        "ch146",
        "ch-146",
        "ch178",
        "ch-178",
        "griffon",
        "raven",
        "mrh90",
        "mrh-90",
        "sa330",
        "sa-330",
        "puma",
        "z8",
        "z-8",
        "z9",
        "z-9",
        "helicopter",
        "heli",
    ]
    .iter()
    .any(|needle| contains_ignore_ascii_case(type_name, needle))
}

pub fn is_deployable_primary_type(type_name: &str) -> bool {
    if contains_ignore_ascii_case(type_name, "sqdeployablechildactor_gen_variable") {
        return false;
    }
    if contains_ignore_ascii_case(type_name, "weapon")
        || contains_ignore_ascii_case(type_name, "baseplate")
        || contains_ignore_ascii_case(type_name, "repairtool")
    {
        return false;
    }
    if starts_with_ignore_ascii_case(type_name, "bp_emplaced") {
        return false;
    }
    contains_ignore_ascii_case(type_name, "fobradio")
        || contains_ignore_ascii_case(type_name, "_hab_")
        || contains_ignore_ascii_case(type_name, "hab_")
        || contains_ignore_ascii_case(type_name, "ammocrate")
        || contains_ignore_ascii_case(type_name, "vehicle_repair")
        || contains_ignore_ascii_case(type_name, "rallypoint")
        || contains_ignore_ascii_case(type_name, "_deployable")
        || contains_ignore_ascii_case(type_name, "_tripod")
        || contains_ignore_ascii_case(type_name, "dshk")
        || contains_ignore_ascii_case(type_name, "kord_tripod")
        || contains_ignore_ascii_case(type_name, "kornet_tripod")
        || contains_ignore_ascii_case(type_name, "spg9_tripod")
        || contains_ignore_ascii_case(type_name, "hj-8atgm_deployable")
        || contains_ignore_ascii_case(type_name, "hj-8atgm_tripod")
        || contains_ignore_ascii_case(type_name, "mk19_tripod")
        || contains_ignore_ascii_case(type_name, "zu-23_emplacement")
}

pub fn is_vehicle_type(type_name: &str) -> bool {
    if is_soldier_type(type_name) || is_deployable_primary_type(type_name) {
        return false;
    }
    for needle in [
        "seat",
        "turret",
        "passenger",
        "weapon",
        "ammowep",
        "smokegenerator",
        "resourceweapon",
        "projectile",
        "commander",
        "cupola",
        "doorgun",
        "doorgun",
        "launcher",
        "destruction",
        "turret1",
        "turret2",
        "turret3",
        "cmdr",
        "pintle",
        "commander_turret",
    ] {
        if contains_ignore_ascii_case(type_name, needle) {
            return false;
        }
    }
    if is_helicopter_type(type_name) {
        return true;
    }
    for needle in [
        // === MAIN BATTLE TANKS ===
        "m1a1",        // US M1A1 Abrams
        "m1a2",        // US M1A2 Abrams
        "abrams",      // US Abrams (general)
        "t72",         // Russian T-72 variants
        "t62",         // Russian T-62
        "t64",         // Ukrainian T-64BM2
        "t90",         // Russian T-90A
        "fv4034",      // British Challenger 2
        "challenger",  // British Challenger
        "leopard",     // German Leopard 2A6M
        "m60",         // Turkish M60T
        "ztz99",       // Chinese ZTZ99A
        
        // === MOBILE GUN SYSTEMS ===
        "m1128",       // US M1128 Stryker MGS
        "sprut",       // Russian Sprut-SDM1
        "ztd05",       // Chinese ZTD05
        
        // === RECONNAISSANCE VEHICLES ===
        "coyote",      // Canadian Coyote
        "fv107",       // British FV107 Scimitar
        
        // === INFANTRY FIGHTING VEHICLES ===
        "bmp",         // Russian BMP series
        "bmd",         // Russian BMD series (airborne)
        "btr82",       // Russian BTR-82A
        "fv510",       // British Warrior
        "fv520",       // British Warrior ATGM
        "warrior",     // British Warrior
        "m2a3",        // US M2A3 Bradley
        "bradley",     // US Bradley
        "lav25",       // US/Canadian LAV-25
        "lav-25",      // US/Canadian LAV-25
        "aslav",       // Australian ASLAV
        "lav6",        // Canadian LAV 6.0
        "lav_6",       // Canadian LAV 6.0
        "acv15",       // Turkish ACV-15
        "acv-15",      // Turkish ACV-15
        "pars",        // Turkish PARS III
        "zbd04",       // Chinese ZBD04A
        "zbd05",       // Chinese ZBD05
        "zbd",         // Chinese ZBD series
        "zbl08",       // Chinese ZBL08
        "zbl",         // Chinese ZBL series
        "zsd89",       // Chinese ZSD89
        "zsd",         // Chinese ZSD series
        "zsl92",       // Chinese ZSL92
        "zsl",         // Chinese ZSL series
        
        // === ARMORED PERSONNEL CARRIERS ===
        "aavp",        // US AAVP
        "aavc",        // US AAVC
        "btr",         // Russian BTR series
        "fv432",       // British FV432
        "lav",         // LAV variants
        "m1126",       // US M1126 Stryker
        "stryker",     // US Stryker
        "m113",        // US M113A3
        "tlav",        // Canadian TLAV
        "mtlb",        // Russian MT-LB
        "mt-lb",       // Russian MT-LB
        "zsl10",       // Chinese ZSL10
        
        // === SCOUT CARS & LIGHT VEHICLES ===
        "m1151",       // US M1151 (HMMWV)
        "humvee",      // HMMWV
        "matv",        // US M-ATV
        "m-atv",       // US M-ATV
        "brdm",        // Russian BRDM-2
        "cobra",       // Turkish Cobra-II
        "csk131",      // Chinese CSK131
        "kozak",       // Ukrainian Kozak-2M1
        "lppv",        // British LPPV
        "luv",         // US LUV-A1
        "luvw",        // Canadian LUVW
        "lynx",        // Australian Lynx
        "pmv",         // Australian PMV Bushmaster
        "bushmaster",  // Australian PMV Bushmaster
        "simir",       // Iranian Simir
        "safir",       // Iranian Safir
        "tapv",        // Canadian TAPV
        "technical",   // Insurgent Technical
        "tigr",        // Russian GAZ Tigr
        "mrap",        // MRAP vehicles (general)
        
        // === TRANSPORT & LOGISTICS TRUCKS ===
        "kamaz",       // Russian KamAZ 5350
        "kraz",        // Ukrainian KrAZ-6322
        "ural",        // Russian Ural trucks
        "m939",        // US M939 truck
        "man_hx",      // German MAN HX
        "manhx",       // German MAN HX
        "man_truck",   // German MAN truck
        "msvs",        // Canadian MSVS
        "ctm131",      // Chinese CTM131
        "ctm",         // Chinese CTM trucks
        "bmc",         // Turkish BMC-185
        "truck",       // General truck
        "logistics",   // Logistics vehicles
        "logi",        // Logi shorthand
        "util_truck",  // Utility truck
        
        // === ARTILLERY ===
        "bm21",        // Russian BM-21 Grad
        "bm-21",       // Russian BM-21 Grad
        "grad",        // BM-21 Grad
        "m1064",       // US M1064A3 mortar carrier
        
        // === MOTORBIKES ===
        "minsk",       // Minsk 400
        "quadbike",    // Quad bike
        "quad_bike",   // Quad bike
        "motorbike",   // Motorbike
        
        // === BOATS ===
        "rhib",        // RHIB boat
        "boat",        // Boats
        
        // === GENERAL PATTERNS ===
        "tank",        // Tanks
        "apc",         // APCs
        "ifv",         // IFVs
        "heli",        // Helicopters
        "vehicle",     // General vehicle (in path like /Game/Vehicles/)
    ] {
        if contains_ignore_ascii_case(type_name, needle) {
            return true;
        }
    }
    false
}

/// Returns `true` if `type_name` looks like a capture zone class.
pub fn is_capture_zone_type(type_name: &str) -> bool {
    contains_ignore_ascii_case(type_name, "capturezone")
        || contains_ignore_ascii_case(type_name, "sqcapturezone")
}

pub fn classify_deployable_event_type(type_name: &str) -> &'static str {
    if contains_ignore_ascii_case(type_name, "fobradio") {
        "RADIO"
    } else if contains_ignore_ascii_case(type_name, "_hab_")
        || contains_ignore_ascii_case(type_name, "hab_")
    {
        "HAB"
    } else if contains_ignore_ascii_case(type_name, "rallypoint") {
        "RALLY"
    } else if contains_ignore_ascii_case(type_name, "ammocrate") {
        "AMMO"
    } else if contains_ignore_ascii_case(type_name, "vehicle_repair") {
        "REPAIR"
    } else if contains_ignore_ascii_case(type_name, "mortar") {
        "MORTAR"
    } else if [
        "tripod",
        "dshk",
        "kord",
        "kornet",
        "spg9",
        "tow",
        "hj-8",
        "mk19",
        "zu-23",
        "emplacement",
    ]
    .iter()
    .any(|needle| contains_ignore_ascii_case(type_name, needle))
    {
        "EMPLACEMENT"
    } else {
        "DEPLOYABLE"
    }
}

pub fn infer_component_type_name(group_path: &str, path_hint: Option<&str>) -> &'static str {
    if contains_ignore_ascii_case(group_path, "sqrotorcomponent")
        || path_hint.is_some_and(|hint| contains_ignore_ascii_case(hint, "rotor"))
    {
        "rotor"
    } else if contains_ignore_ascii_case(group_path, "sqvehicletrack")
        || path_hint.is_some_and(|hint| contains_ignore_ascii_case(hint, "track"))
    {
        "track"
    } else if contains_ignore_ascii_case(group_path, "sqvehicleammobox")
        || path_hint.is_some_and(|hint| contains_ignore_ascii_case(hint, "ammorack"))
    {
        "ammorack"
    } else if contains_ignore_ascii_case(group_path, "sqvehiclewheel")
        || path_hint.is_some_and(|hint| contains_ignore_ascii_case(hint, "wheel"))
    {
        "wheel"
    } else if contains_ignore_ascii_case(group_path, "sqvehicleseatcomponent")
        || path_hint.is_some_and(|hint| contains_ignore_ascii_case(hint, "seat"))
    {
        "seat"
    } else {
        "component"
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn infer_component_type(group_path: &str, path_hint: Option<&str>) -> String {
    infer_component_type_name(group_path, path_hint).to_string()
}

pub fn infer_group_leaf(path: &str) -> &str {
    if let Some(idx) = path.rfind('.') {
        return &path[idx + 1..];
    }
    if let Some(idx) = path.rfind('/') {
        return &path[idx + 1..];
    }
    path
}

/// Try to extract a RAAS/AAS flag name from a path string.
/// Looks for patterns like "C1-Diefenbunker", "B3-CityCenter" etc.
/// Returns Some((lane_letter, lane_number, display_name)) if found.
/// 
/// Only returns flags that appear to be active (instantiated with DefaultSceneRoot).
pub fn extract_raas_flag_from_path(path: &str) -> Option<(char, u8, String)> {
    // Only extract flags that are actual scene instances (have DefaultSceneRoot)
    // This filters out inactive lane flags that are just map data references
    if !path.contains("DefaultSceneRoot") {
        return None;
    }
    
    // Look for patterns after "PersistentLevel"
    let after_level = if let Some(idx) = path.find("PersistentLevel") {
        &path[idx + "PersistentLevel".len()..]
    } else {
        path
    };
    
    // Pattern: single letter (A-Z) followed by digit followed by hyphen followed by name
    // e.g., "C1-Diefenbunker", "B3-CityCenter", "A2-Village"
    let chars: Vec<char> = after_level.chars().collect();
    if chars.len() < 4 {
        return None;
    }
    
    // First char must be A-Z
    let lane_letter = chars[0];
    if !lane_letter.is_ascii_uppercase() {
        return None;
    }
    
    // Second char must be digit
    let lane_digit = chars[1];
    if !lane_digit.is_ascii_digit() {
        return None;
    }
    let lane_number = lane_digit.to_digit(10)? as u8;
    
    // Third char must be hyphen
    if chars[2] != '-' {
        return None;
    }
    
    // Extract the flag name (everything after the hyphen until special chars or end)
    let name_start = 3;
    let mut name_end = chars.len();
    for (i, c) in chars[name_start..].iter().enumerate() {
        if *c == '?' || *c == '.' || *c == '/' || !c.is_ascii_alphanumeric() {
            name_end = name_start + i;
            break;
        }
    }
    
    if name_end <= name_start {
        return None;
    }
    
    let flag_name: String = chars[name_start..name_end].iter().collect();
    let display_name = format!("{}{}-{}", lane_letter, lane_number, flag_name);
    
    Some((lane_letter, lane_number, display_name))
}

/// Check if a path contains a RAAS/AAS capture zone flag reference.
pub fn is_raas_flag_path(path: &str) -> bool {
    extract_raas_flag_from_path(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::{infer_component_type, is_helicopter_type, is_vehicle_type, extract_raas_flag_from_path};

    #[test]
    fn raas_flag_extraction_works() {
        // Test with full path containing DefaultSceneRoot (active flag)
        let result = extract_raas_flag_from_path(
            "/Game/Maps/Manicouagan/PersistentLevelC1-Diefenbunker?DefaultSceneRoot"
        );
        assert!(result.is_some());
        let (lane, num, name) = result.unwrap();
        assert_eq!(lane, 'C');
        assert_eq!(num, 1);
        assert_eq!(name, "C1-Diefenbunker");
        
        // Test with another active flag
        let result = extract_raas_flag_from_path("PersistentLevelB3-Village?DefaultSceneRoot?SQCaptureZone");
        assert!(result.is_some());
        let (lane, num, name) = result.unwrap();
        assert_eq!(lane, 'B');
        assert_eq!(num, 3);
        assert_eq!(name, "B3-Village");
        
        // Test non-flag paths (no DefaultSceneRoot = inactive, not extracted)
        assert!(extract_raas_flag_from_path("PersistentLevel00-Team1Main").is_none());
        assert!(extract_raas_flag_from_path("/Script/Squad.SQCaptureZone").is_none());
        // Inactive lane flags (no DefaultSceneRoot) should not be extracted
        assert!(extract_raas_flag_from_path("PersistentLevelA1-LoggingCamp").is_none());
    }

    #[test]
    fn vehicle_seat_components_are_classified_as_seats() {
        assert_eq!(
            infer_component_type("/Script/Squad.SQVehicleSeatComponent", None),
            "seat"
        );
    }

    #[test]
    fn helicopter_classification_covers_current_families() {
        for type_name in [
            "BP_UH60M_C",
            "BP_CH146_Utility_C",
            "BP_CH178_Transport_C",
            "BP_Mi8MTV5_C",
            "BP_MRH90_C",
            "BP_SA330_C",
            "BP_Z8G_C",
            "BP_Z9A_C",
        ] {
            assert!(
                is_helicopter_type(type_name),
                "{type_name} should be a helicopter"
            );
            assert!(
                is_vehicle_type(type_name),
                "{type_name} should be a vehicle"
            );
        }
    }
}
