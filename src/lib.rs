//! plato-deadband — The Deadband Protocol
//!
//! Fleet doctrine: P0 (map negative space), P1 (find safe channels), P2 (optimize).
//! Strict priority. Never skip to P2.
//!
//! ## The Insight (Oracle1)
//! - 20×20 maze: greedy 0/50, deadband 50/50
//! - 30×30 fleet sim: greedy 0/30, deadband 30/30
//! - Applies to: navigation, code, research, training, agents
//!
//! ## Mapping to Fleet Crates
//! - P0 = plato-lab-guard (constraints), plato-dcs locks, negative space tiles
//! - P1 = plato-ghostable (persistence), plato-relay (trust routing), plato-tile-search
//! - P2 = plato-tile-scorer (optimization), plato-room-engine (execution)
//!
//! ## API
//! ```rust
//! let db = DeadbandEngine::new();
//! db.learn_negative("never rm -rf /");         // P0
//! db.mark_channel(vec!["safe-path-1"]);       // P1
//! db.optimize("fastest route via safe-path");  // P2
//! ```

use std::collections::HashMap;

/// Priority levels in the deadband protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Map negative space — what NOT to do. The rocks.
    P0,
    /// Find safe channels — where you CAN be. The water.
    P1,
    /// Optimize within channels — best path. The course.
    P2,
}

impl Default for Priority { fn default() -> Self { Priority::P1 } }

/// A negative-space entry (something NOT to do).
#[derive(Debug, Clone)]
pub struct NegativeSpace {
    pub id: String,
    pub pattern: String,
    pub reason: String,
    pub severity: f64,    // 0.0 = mild, 1.0 = catastrophic
    pub source: String,   // which agent/crate reported this
    pub confirmed: u32,   // how many times confirmed
    pub violated: u32,    // how many times violated (should be 0)
}

/// A safe channel (somewhere you CAN be).
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub description: String,
    pub priority: Priority,
    pub confidence: f64,  // 0.0 = uncertain, 1.0 = proven
    pub used_count: u32,
}

/// A P2 optimization within a channel.
#[derive(Debug, Clone)]
pub struct Optimization {
    pub id: String,
    pub channel_id: String,
    pub description: String,
    pub improvement: f64, // measured improvement over baseline
}

/// Result of a deadband check.
#[derive(Debug, Clone)]
pub struct DeadbandCheck {
    pub passed: bool,
    pub violated_priority: Option<Priority>,
    pub violations: Vec<String>,
    pub recommended_channel: Option<String>,
    pub p0_clear: bool,
    pub p1_clear: bool,
}

/// The deadband engine.
pub struct DeadbandEngine {
    negatives: HashMap<String, NegativeSpace>,
    channels: HashMap<String, Channel>,
    optimizations: HashMap<String, Optimization>,
}

impl Default for DeadbandEngine { fn default() -> Self { Self::new() } }

impl DeadbandEngine {
    pub fn new() -> Self {
        Self { negatives: HashMap::new(), channels: HashMap::new(), optimizations: HashMap::new() }
    }

    // ── P0: Map Negative Space ──

    /// Learn a new negative-space entry (something NOT to do).
    pub fn learn_negative(&mut self, id: &str, pattern: &str, reason: &str, severity: f64, source: &str) {
        self.negatives.insert(id.to_string(), NegativeSpace {
            id: id.to_string(), pattern: pattern.to_string(), reason: reason.to_string(),
            severity: severity.max(0.0).min(1.0), source: source.to_string(), confirmed: 1, violated: 0,
        });
    }

    /// Confirm an existing negative-space entry.
    pub fn confirm_negative(&mut self, id: &str) -> bool {
        if let Some(neg) = self.negatives.get_mut(id) { neg.confirmed += 1; true } else { false }
    }

    /// Check if an action violates any P0 constraints.
    pub fn check_p0(&self, action: &str) -> Vec<&NegativeSpace> {
        let action_lower = action.to_lowercase();
        self.negatives.values().filter(|neg| {
            action_lower.contains(&neg.pattern.to_lowercase())
        }).collect()
    }

    /// P0 clear? (no violations)
    pub fn p0_clear(&self, action: &str) -> bool {
        self.check_p0(action).is_empty()
    }

    // ── P1: Find Safe Channels ──

    /// Register a safe channel.
    pub fn mark_channel(&mut self, id: &str, description: &str, confidence: f64) {
        self.channels.insert(id.to_string(), Channel {
            id: id.to_string(), description: description.to_string(),
            priority: Priority::P1, confidence: confidence.max(0.0).min(1.0), used_count: 0,
        });
    }

    /// Find channels safe for the given action.
    pub fn find_channels(&self, action: &str) -> Vec<&Channel> {
        let action_lower = action.to_lowercase();
        self.channels.values()
            .filter(|ch| {
                let desc_lower = ch.description.to_lowercase();
                let id_lower = ch.id.to_lowercase();
                desc_lower.contains(&action_lower) || action_lower.contains(&desc_lower)
                    || action_lower.contains(&id_lower) || id_lower.contains(&action_lower)
            })
            .collect()
    }

    /// Use a channel (increments usage count).
    pub fn use_channel(&mut self, id: &str) -> bool {
        if let Some(ch) = self.channels.get_mut(id) { ch.used_count += 1; true } else { false }
    }

    /// P1 clear? (at least one safe channel found)
    pub fn p1_clear(&self, action: &str) -> bool {
        !self.find_channels(action).is_empty()
    }

    // ── P2: Optimize Within Channels ──

    /// Register a P2 optimization within a channel.
    pub fn optimize(&mut self, id: &str, channel_id: &str, description: &str, improvement: f64) {
        self.optimizations.insert(id.to_string(), Optimization {
            id: id.to_string(), channel_id: channel_id.to_string(),
            description: description.to_string(), improvement,
        });
    }

    /// Get best optimization for a channel.
    pub fn best_optimization(&self, channel_id: &str) -> Option<&Optimization> {
        let mut best: Option<&Optimization> = None;
        for opt in self.optimizations.values() {
            if opt.channel_id == channel_id {
                if best.map_or(true, |b| opt.improvement > b.improvement) {
                    best = Some(opt);
                }
            }
        }
        best
    }

    // ── Full Deadband Check ──

    /// Full deadband check: P0 → P1 → P2.
    /// Returns check result with violations and recommendations.
    pub fn check(&self, action: &str) -> DeadbandCheck {
        let p0_violations = self.check_p0(action);
        let p0_clear = p0_violations.is_empty();

        let channels = self.find_channels(action);
        let p1_clear = !channels.is_empty();
        let recommended_channel = channels.first().map(|ch| ch.id.clone());

        let best_opt = recommended_channel.as_ref().and_then(|ch| self.best_optimization(ch));

        let passed = p0_clear && p1_clear;
        let violated_priority = if !p0_clear { Some(Priority::P0) }
            else if !p1_clear { Some(Priority::P1) }
            else { None };

        DeadbandCheck {
            passed,
            violated_priority,
            violations: p0_violations.iter().map(|v| v.pattern.clone()).collect(),
            recommended_channel,
            p0_clear,
            p1_clear,
        }
    }

    /// Execute action through deadband (returns result + whether it was allowed).
    pub fn execute(&mut self, action: &str) -> Result<DeadbandCheck, String> {
        let check = self.check(action);
        if !check.p0_clear {
            return Err(format!("P0 VIOLATION: {}", check.violations.join(", ")));
        }
        if !check.p1_clear {
            return Err("P1 BLOCKED: no safe channel found".to_string());
        }
        if let Some(ref ch) = check.recommended_channel {
            self.use_channel(ch);
        }
        Ok(check)
    }

    // ── Metrics ──

    pub fn negative_count(&self) -> usize { self.negatives.len() }
    pub fn channel_count(&self) -> usize { self.channels.len() }
    pub fn optimization_count(&self) -> usize { self.optimizations.len() }
    pub fn total_confirmed(&self) -> u32 { self.negatives.values().map(|n| n.confirmed).sum() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learn_negative() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("rm-rf", "rm -rf /", "destroys filesystem", 1.0, "fleet");
        assert_eq!(db.negative_count(), 1);
    }

    #[test]
    fn test_p0_clear() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("rm-rf", "rm -rf /", "destroys", 1.0, "fleet");
        assert!(db.p0_clear("echo hello"));
        assert!(!db.p0_clear("rm -rf /tmp"));
    }

    #[test]
    fn test_p0_violation_list() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("rm", "rm -rf", "dangerous", 1.0, "fleet");
        db.learn_negative("delete", "DELETE FROM", "data loss", 0.9, "fleet");
        let violations = db.check_p0("rm -rf / && DELETE FROM users");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn test_mark_and_find_channel() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("safe-math", "math operations", 0.9);
        db.mark_channel("safe-io", "file reading", 0.8);
        let channels = db.find_channels("math");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "safe-math");
    }

    #[test]
    fn test_p1_clear() {
        let mut db = DeadbandEngine::new();
        assert!(!db.p1_clear("anything"));
        db.mark_channel("ch1", "anything related", 0.5);
        assert!(db.p1_clear("anything"));
    }

    #[test]
    fn test_use_channel() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("ch1", "test", 0.5);
        assert!(db.use_channel("ch1"));
        assert!(!db.use_channel("nonexistent"));
        assert_eq!(db.channels.get("ch1").unwrap().used_count, 1);
    }

    #[test]
    fn test_optimize() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("ch1", "math", 0.9);
        db.optimize("opt1", "ch1", "use lookup table", 0.3);
        db.optimize("opt2", "ch1", "cache results", 0.5);
        let best = db.best_optimization("ch1").unwrap();
        assert_eq!(best.id, "opt2");
    }

    #[test]
    fn test_full_check_passed() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("safe", "query tiles about", 0.9);
        let check = db.check("query tiles about math");
        assert!(check.passed);
        assert!(check.p0_clear);
        assert!(check.p1_clear);
    }

    #[test]
    fn test_full_check_p0_failed() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("danger", "rm -rf", "destroy", 1.0, "fleet");
        let check = db.check("rm -rf /home");
        assert!(!check.passed);
        assert!(!check.p0_clear);
        assert_eq!(check.violated_priority, Some(Priority::P0));
    }

    #[test]
    fn test_full_check_p1_failed() {
        let db = DeadbandEngine::new();
        let check = db.check("some random action");
        assert!(!check.passed);
        assert!(check.p0_clear); // no negatives defined
        assert!(!check.p1_clear); // no channels
        assert_eq!(check.violated_priority, Some(Priority::P1));
    }

    #[test]
    fn test_execute_success() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("ch", "test action", 0.9);
        let result = db.execute("test action");
        assert!(result.is_ok());
        assert_eq!(db.channels.get("ch").unwrap().used_count, 1);
    }

    #[test]
    fn test_execute_p0_blocked() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("x", "bad thing", "danger", 1.0, "fleet");
        let result = db.execute("do the bad thing");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("P0"));
    }

    #[test]
    fn test_execute_p1_blocked() {
        let mut db = DeadbandEngine::new();
        let result = db.execute("orphan action");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("P1"));
    }

    #[test]
    fn test_confirm_negative() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("x", "pattern", "reason", 0.5, "source");
        db.confirm_negative("x");
        assert_eq!(db.total_confirmed(), 2); // 1 initial + 1 confirm
    }

    #[test]
    fn test_confirm_nonexistent() {
        let mut db = DeadbandEngine::new();
        assert!(!db.confirm_negative("ghost"));
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::P0 < Priority::P1);
        assert!(Priority::P1 < Priority::P2);
    }

    #[test]
    fn test_case_insensitive_p0() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("x", "RM -RF", "danger", 1.0, "fleet");
        assert!(!db.p0_clear("rm -rf /tmp")); // lowercase query matches uppercase pattern
    }

    #[test]
    fn test_severity_clamped() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("x", "p", "r", 5.0, "s"); // way over 1.0
        assert!((db.negatives.get("x").unwrap().severity - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_metrics() {
        let mut db = DeadbandEngine::new();
        db.learn_negative("a", "x", "y", 0.5, "s");
        db.learn_negative("b", "z", "w", 0.5, "s");
        db.mark_channel("c1", "desc", 0.5);
        db.optimize("o1", "c1", "desc", 0.3);
        assert_eq!(db.negative_count(), 2);
        assert_eq!(db.channel_count(), 1);
        assert_eq!(db.optimization_count(), 1);
    }

    #[test]
    fn test_recommended_channel() {
        let mut db = DeadbandEngine::new();
        db.mark_channel("math-ch", "math operations", 0.9);
        db.mark_channel("io-ch", "file operations", 0.8);
        let check = db.check("math operations");
        assert_eq!(check.recommended_channel.as_deref(), Some("math-ch"));
    }

    #[test]
    fn test_best_opt_for_nonexistent_channel() {
        let db = DeadbandEngine::new();
        assert!(db.best_optimization("ghost").is_none());
    }
}
