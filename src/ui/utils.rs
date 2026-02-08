

pub fn format_version_range(v: &str) -> String {
    if v == "*" { return "*".to_string(); }
    
    // Strip operators to isolate versions
    let s = v.replace(">=", "")
             .replace("<=", "")
             .replace(">", "")
             .replace("<", "");

    // Split by whitespace to process individual version segments
    let parts: Vec<String> = s.split_whitespace()
        .map(|part| {
            let mut p = part.to_string();
            // User requested removal of trailing hyphens (e.g. "1.21.10-" -> "1.21.10")
            if p.ends_with('-') { p.pop(); }
            p
        })
        .collect();

    if parts.is_empty() { return v.to_string(); }

    if parts.len() >= 2 {
        // It's likely a range: "1.21.10" "1.21.11" -> "1.21.10 - 1.21.11"
        return parts.join(" - ");
    } else {
        // Single version constraint
        let val = &parts[0];
        if v.contains('>') {
            return format!("{} +", val);
        } else if v.contains('<') {
            return format!("{} -", val);
        } else {
            return val.clone();
        }
    }
}

pub fn format_dep_name(k: &str) -> String {
    match k {
        "fabricloader" => "Fabric".to_string(),
        "fabric-loader" => "Fabric".to_string(),
        "forge" => "Forge".to_string(),
        "neoforge" => "NeoForge".to_string(),
        "quilt_loader" => "Quilt".to_string(),
        "minecraft" => "Minecraft".to_string(),
        "java" => "Java".to_string(),
        "fabric-api" => "Fabric API".to_string(),
        _ => k.to_string(), // fallback
    }
}

pub fn calculate_worker_count(task_count: usize) -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    
    // Dynamic worker calculation:
    // - Small mod counts: 1 worker per mod (min(mods, max_workers))
    // - Large mod counts: Up to 8 workers per cpu, capped at 64
    let max_workers = (cpus * 8).clamp(4, 64);
    std::cmp::min(task_count, max_workers).max(1)
}