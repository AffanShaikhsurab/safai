//! What an autopilot run is allowed to delete.
//!
//! This is the most safety-critical file in the automation feature: it decides
//! which scan findings may be removed with nobody watching. The filter is
//! deliberately a **whitelist with hard-coded floors** — a hand-edited config
//! file cannot widen it past what the code permits.
//!
//! An item has to clear every one of these to be eligible:
//!
//! 1. Tier is **not** `Caution` — enforced here, not just in config, because
//!    `Caution` covers the user's own data (downloaded models, editor state).
//! 2. Tier is in the user's `auto_clean_tiers` (default: `Safe` only).
//! 3. Category is in the user's `auto_clean_categories`.
//! 4. `regenerates == true` — the tool that created it will recreate it. If a
//!    rule can't promise that, autopilot won't touch it.
//! 5. Not a generic large-folder discovery hit. Those are "here's something
//!    big, we don't know what it is" findings; they are never auto-removable.
//! 6. If the user narrowed things to specific rule ids, it's one of them.
//!
//! Whatever survives is then sorted largest-first and accumulated up to
//! `max_auto_clean_bytes`, so a capped run still frees the most space it can.

use safai_rules::{CleanupItem, SafetyTier, ScanReport};

use super::config::ScheduleConfig;

/// `rule_id` used by the scan's generic "this folder is big" discovery pass.
/// Never eligible for automatic deletion.
const DISCOVERY_RULE_ID: &str = "large-folder";

/// The set of items an autopilot run will delete, plus why the rest were left.
#[derive(Debug, Clone, Default)]
pub struct AutoCleanPlan {
    /// Items cleared for automatic deletion, largest first.
    pub items: Vec<CleanupItem>,
    /// Sum of `items`' sizes.
    pub total_bytes: u64,
    /// Findings the policy filter excluded.
    pub excluded_by_policy: u32,
    /// Findings that passed the filter but didn't fit under the byte cap.
    pub excluded_by_cap: u32,
}

impl AutoCleanPlan {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Is this single item eligible for automatic deletion under `cfg`?
pub fn is_auto_eligible(item: &CleanupItem, cfg: &ScheduleConfig) -> bool {
    // (1) Absolute floor — user data is never auto-removed.
    if item.tier == SafetyTier::Caution {
        return false;
    }
    // (2) + (3) Within the user's chosen tiers and categories.
    if !cfg.auto_clean_tiers.contains(&item.tier) {
        return false;
    }
    if !cfg.auto_clean_categories.contains(&item.category) {
        return false;
    }
    // (4) Must come back on its own.
    if !item.regenerates {
        return false;
    }
    // (5) Never the "big unknown folder" findings.
    if item.rule_id == DISCOVERY_RULE_ID {
        return false;
    }
    // (6) Optional narrowing to specific rules.
    if !cfg.auto_clean_rule_ids.is_empty() && !cfg.auto_clean_rule_ids.contains(&item.rule_id) {
        return false;
    }
    true
}

/// Build the autopilot deletion plan for a finished scan.
pub fn select_auto_clean(report: &ScanReport, cfg: &ScheduleConfig) -> AutoCleanPlan {
    let mut plan = AutoCleanPlan::default();

    let mut eligible: Vec<CleanupItem> = Vec::new();
    for group in &report.groups {
        for item in &group.items {
            if is_auto_eligible(item, cfg) {
                eligible.push(item.clone());
            } else {
                plan.excluded_by_policy += 1;
            }
        }
    }

    // Largest first, so a capped run reclaims as much as it can.
    eligible.sort_by_key(|item| std::cmp::Reverse(item.size_bytes));

    for item in eligible {
        let next_total = plan.total_bytes.saturating_add(item.size_bytes);
        if next_total > cfg.max_auto_clean_bytes {
            plan.excluded_by_cap += 1;
            continue;
        }
        plan.total_bytes = next_total;
        plan.items.push(item);
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use safai_rules::{Category, CategoryGroup};

    fn item(
        id: &str,
        rule_id: &str,
        category: Category,
        tier: SafetyTier,
        size_bytes: u64,
        regenerates: bool,
    ) -> CleanupItem {
        CleanupItem {
            id: id.to_string(),
            rule_id: rule_id.to_string(),
            label: id.to_string(),
            category,
            tier,
            path: format!("C:/test/{id}"),
            size_bytes,
            regenerates,
            last_modified_secs: None,
            note: String::new(),
            selected_by_default: tier == SafetyTier::Safe,
        }
    }

    fn report(items: Vec<CleanupItem>) -> ScanReport {
        ScanReport {
            total_reclaimable_bytes: items.iter().map(|i| i.size_bytes).sum(),
            groups: vec![CategoryGroup {
                category: Category::PackageCache,
                label: "Package caches".to_string(),
                total_bytes: items.iter().map(|i| i.size_bytes).sum(),
                items,
            }],
            scanned_roots: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn default_policy_takes_safe_regenerating_package_caches() {
        let cfg = ScheduleConfig::default();
        let plan = select_auto_clean(
            &report(vec![item(
                "a",
                "npm-cache",
                Category::PackageCache,
                SafetyTier::Safe,
                1000,
                true,
            )]),
            &cfg,
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.total_bytes, 1000);
    }

    #[test]
    fn caution_tier_is_never_eligible_even_if_config_allows_it() {
        // Simulate a hand-edited config that tries to opt into Caution.
        let cfg = ScheduleConfig {
            auto_clean_tiers: vec![SafetyTier::Safe, SafetyTier::Caution],
            auto_clean_categories: vec![Category::Model, Category::PackageCache],
            ..Default::default()
        };

        let plan = select_auto_clean(
            &report(vec![item(
                "models",
                "lmstudio-models",
                Category::Model,
                SafetyTier::Caution,
                9_000_000,
                false,
            )]),
            &cfg,
        );
        assert!(plan.is_empty());
        assert_eq!(plan.excluded_by_policy, 1);
    }

    #[test]
    fn non_regenerating_items_are_excluded() {
        let cfg = ScheduleConfig {
            auto_clean_categories: vec![Category::PackageCache],
            ..Default::default()
        };
        let plan = select_auto_clean(
            &report(vec![item(
                "a",
                "npm-cache",
                Category::PackageCache,
                SafetyTier::Safe,
                10,
                false,
            )]),
            &cfg,
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn generic_discovery_hits_are_excluded() {
        let cfg = ScheduleConfig {
            auto_clean_categories: vec![Category::PackageCache],
            ..Default::default()
        };
        let plan = select_auto_clean(
            &report(vec![item(
                "big",
                "large-folder",
                Category::PackageCache,
                SafetyTier::Safe,
                10,
                true,
            )]),
            &cfg,
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn categories_outside_the_whitelist_are_excluded() {
        let cfg = ScheduleConfig::default(); // no BuildArtifact by default
        let plan = select_auto_clean(
            &report(vec![item(
                "nm",
                "node-modules",
                Category::BuildArtifact,
                SafetyTier::Safe,
                500,
                true,
            )]),
            &cfg,
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn rule_id_narrowing_is_respected() {
        let cfg = ScheduleConfig {
            auto_clean_rule_ids: vec!["npm-cache".to_string()],
            ..Default::default()
        };
        let plan = select_auto_clean(
            &report(vec![
                item(
                    "a",
                    "npm-cache",
                    Category::PackageCache,
                    SafetyTier::Safe,
                    10,
                    true,
                ),
                item(
                    "b",
                    "pip-cache",
                    Category::PackageCache,
                    SafetyTier::Safe,
                    20,
                    true,
                ),
            ]),
            &cfg,
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].rule_id, "npm-cache");
    }

    #[test]
    fn byte_cap_keeps_the_biggest_items_that_fit() {
        // 1 MiB is the floor `sanitized` clamps to.
        let cfg = ScheduleConfig {
            max_auto_clean_bytes: 1024 * 1024,
            ..Default::default()
        };
        let plan = select_auto_clean(
            &report(vec![
                item(
                    "big",
                    "npm-cache",
                    Category::PackageCache,
                    SafetyTier::Safe,
                    900_000,
                    true,
                ),
                item(
                    "huge",
                    "pip-cache",
                    Category::PackageCache,
                    SafetyTier::Safe,
                    5_000_000,
                    true,
                ),
                item(
                    "small",
                    "uv-cache",
                    Category::PackageCache,
                    SafetyTier::Safe,
                    100_000,
                    true,
                ),
            ]),
            &cfg,
        );
        // "huge" alone busts the cap and is skipped; the other two fit.
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.excluded_by_cap, 1);
        assert!(plan.total_bytes <= cfg.max_auto_clean_bytes);
        assert_eq!(plan.items[0].id, "big", "largest that fits comes first");
    }
}
