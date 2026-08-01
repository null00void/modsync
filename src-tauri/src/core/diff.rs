use super::mods_yml::ModEntry;
use super::supabase_client::SyncedModEntry;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct SyncPlanEntry {
    pub full_name: String,
    pub display_name: String,
    pub author_name: String,
    pub local_version: Option<String>,
    pub friend_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncPlan {
    /// Friend has it, local doesn't have it at all.
    pub to_install: Vec<SyncPlanEntry>,
    /// Both have it, but at different versions (target = friend's version).
    pub to_update: Vec<SyncPlanEntry>,
    /// Local currently has it enabled, but the friend either doesn't have
    /// it or has it disabled -- gets turned off, not deleted.
    pub to_disable: Vec<SyncPlanEntry>,
    /// Local currently has it disabled, friend has it enabled at the same
    /// version -- just needs re-enabling, no download required.
    pub to_reenable: Vec<SyncPlanEntry>,
    /// Same version, same enabled state on both sides -- nothing to do.
    pub unchanged: Vec<SyncPlanEntry>,
}

struct LocalState {
    version: String,
    enabled: bool,
    display_name: String,
    author_name: String,
}

/// Compares the local profile's mods (only the online-sourced ones --
/// matches exactly what build_push_payload would have sent) against a
/// friend's fetched payload, producing the plan a "sync from this
/// friend" action would execute.
pub fn diff_against_friend(local: &[ModEntry], friend: &[SyncedModEntry]) -> SyncPlan {
    let mut local_map: HashMap<&str, LocalState> = HashMap::new();
    for entry in local.iter().filter(|e| e.online_source) {
        local_map.insert(
            entry.name.as_str(),
            LocalState {
                version: entry.version_number.to_string(),
                enabled: entry.enabled,
                display_name: entry.display_name.clone(),
                author_name: entry.author_name.clone(),
            },
        );
    }

    let mut plan = SyncPlan::default();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for f in friend {
        seen.insert(f.full_name.as_str());
        let entry_base = SyncPlanEntry {
            full_name: f.full_name.clone(),
            display_name: f.display_name.clone(),
            author_name: f.author_name.clone(),
            local_version: local_map.get(f.full_name.as_str()).map(|l| l.version.clone()),
            friend_version: Some(f.version.clone()),
        };

        match local_map.get(f.full_name.as_str()) {
            None => plan.to_install.push(entry_base),
            Some(local) if local.version != f.version => plan.to_update.push(entry_base),
            Some(local) if !local.enabled && f.enabled => plan.to_reenable.push(entry_base),
            Some(local) if local.enabled && !f.enabled => plan.to_disable.push(entry_base),
            Some(_) => plan.unchanged.push(entry_base),
        }
    }

    // Anything local has, currently enabled, that the friend doesn't
    // list at all -- turn it off (never hard-delete).
    for (full_name, local) in local_map.iter() {
        if seen.contains(full_name) || !local.enabled {
            continue;
        }
        plan.to_disable.push(SyncPlanEntry {
            full_name: full_name.to_string(),
            display_name: local.display_name.clone(),
            author_name: local.author_name.clone(),
            local_version: Some(local.version.clone()),
            friend_version: None,
        });
    }

    plan
}

/// Same comparison as `diff_against_friend`, but shaped for execution
/// rather than display: full `SyncedModEntry` data (version, enabled,
/// dependencies) for anything that needs a fresh install/update, plus
/// plain full_name lists for enable/disable-only changes that don't need
/// any download at all.
pub struct ExecutionPlan {
    pub to_install_or_update: Vec<SyncedModEntry>,
    pub to_disable: Vec<String>,
    pub to_reenable: Vec<String>,
}

pub fn plan_execution(local: &[ModEntry], friend: &[SyncedModEntry]) -> ExecutionPlan {
    let mut local_map: HashMap<&str, LocalState> = HashMap::new();
    for entry in local.iter().filter(|e| e.online_source) {
        local_map.insert(
            entry.name.as_str(),
            LocalState {
                version: entry.version_number.to_string(),
                enabled: entry.enabled,
                display_name: entry.display_name.clone(),
                author_name: entry.author_name.clone(),
            },
        );
    }

    let mut plan = ExecutionPlan {
        to_install_or_update: Vec::new(),
        to_disable: Vec::new(),
        to_reenable: Vec::new(),
    };
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for f in friend {
        seen.insert(f.full_name.as_str());
        match local_map.get(f.full_name.as_str()) {
            None => plan.to_install_or_update.push(f.clone()),
            Some(local) if local.version != f.version => plan.to_install_or_update.push(f.clone()),
            Some(local) if !local.enabled && f.enabled => plan.to_reenable.push(f.full_name.clone()),
            Some(local) if local.enabled && !f.enabled => plan.to_disable.push(f.full_name.clone()),
            Some(_) => {}
        }
    }

    for (full_name, local) in local_map.iter() {
        if seen.contains(full_name) || !local.enabled {
            continue;
        }
        plan.to_disable.push(full_name.to_string());
    }

    plan
}
