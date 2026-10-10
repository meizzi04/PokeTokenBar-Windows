//! Pokémon companion: egg → hatch → evolve → graduate into the Pokédex.
//! Balance is ported from the original `CompanionModel.swift` / `CompanionStore.swift`.

use fastrand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const EGG_HATCH_THRESHOLD: u64 = 5_000_000;
pub const SHINY_DENOMINATOR: u64 = 64;
/// Gen-V animated sprites exist up to #649, so lines are cut there.
pub const MAX_SPECIES_ID: u32 = 649;
pub const DITTO_ID: u32 = 132;

pub const NATURES: [&str; 25] = [
    "hardy", "lonely", "brave", "adamant", "naughty", "bold", "docile", "relaxed", "impish", "lax",
    "timid", "hasty", "serious", "jolly", "naive", "modest", "mild", "quiet", "bashful", "rash",
    "calm", "gentle", "sassy", "careful", "quirky",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    pub fn from(capture_rate: u32, legendary_or_mythical: bool) -> Self {
        match capture_rate {
            _ if legendary_or_mythical => Rarity::Legendary,
            0..=45 => Rarity::Rare,
            46..=120 => Rarity::Uncommon,
            _ => Rarity::Common,
        }
    }

    pub fn graduation_total(self) -> u64 {
        match self {
            Rarity::Common => 750_000_000,
            Rarity::Uncommon => 1_875_000_000,
            Rarity::Rare => 3_000_000_000,
            Rarity::Legendary => 6_000_000_000,
        }
    }
}

/// Tokens needed to leave stage `stage_index` (0-based) of a `k`-form line.
/// Stage i costs T·i / (k(k+1)/2), so every line of a rarity sums to the same total T.
pub fn phase_threshold(rarity: Rarity, k: usize, stage_index: usize) -> u64 {
    let k = k.max(1) as f64;
    let i = (stage_index + 1) as f64;
    let t = rarity.graduation_total() as f64;
    ((t * i / (k * (k + 1.0) / 2.0)).round() as u64).max(1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvoNode {
    pub id: u32,
    pub children: Vec<EvoNode>,
}

impl EvoNode {
    pub fn find(&self, id: u32) -> Option<&EvoNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    pub fn final_ids(&self) -> Vec<u32> {
        if self.children.is_empty() {
            return vec![self.id];
        }
        self.children.iter().flat_map(EvoNode::final_ids).collect()
    }

    pub fn all_ids(&self) -> Vec<u32> {
        let mut v = vec![self.id];
        self.children.iter().for_each(|c| v.extend(c.all_ids()));
        v
    }

    /// Drops species without an animated sprite, and everything after them.
    pub fn keep_animated(&self) -> Option<EvoNode> {
        (self.id <= MAX_SPECIES_ID).then(|| EvoNode {
            id: self.id,
            children: self.children.iter().filter_map(EvoNode::keep_animated).collect(),
        })
    }
}

/// species id → language code → name
pub type Names = HashMap<u32, HashMap<String, String>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub base_id: u32,
    pub tree: EvoNode,
    pub rarity: Rarity,
    pub names: Names,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mon {
    pub line: Line,
    /// Realized evolution path so far.
    pub path_ids: Vec<u32>,
    /// Full route chosen at hatch (branches resolved up front).
    pub planned_ids: Vec<u32>,
    pub stage_index: usize,
    pub used_at_stage: u64,
    pub is_shiny: bool,
    pub nature: String,
}

impl Mon {
    pub fn current_id(&self) -> u32 {
        self.path_ids[self.stage_index.min(self.path_ids.len() - 1)]
    }
    pub fn threshold(&self) -> u64 {
        phase_threshold(self.line.rarity, self.planned_ids.len(), self.stage_index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexEntry {
    pub base_id: u32,
    pub final_id: u32,
    pub chain: Vec<u32>,
    pub rarity: Rarity,
    pub is_shiny: bool,
    pub nature: String,
    pub names: Names,
    pub caught_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    Hatched { id: u32, shiny: bool },
    Evolved { id: u32 },
    Graduated { id: u32 },
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Usage before the first observation isn't counted (no retroactive growth).
    pub baseline_set: bool,
    /// provider → today's tokens already credited.
    pub ledger: HashMap<String, u64>,
    pub last_date: String,
    pub used_since_install: u64,
    pub egg_usage: u64,
    pub active: Option<Mon>,
    pub dex: Vec<DexEntry>,
    /// "base:final" pairs, used to prefer new branches and down-weight repeats.
    pub collected_finals: HashSet<String>,
}

impl State {
    /// Credits today's per-provider growth since the last call. Returns the new tokens.
    pub fn credit(&mut self, today: &HashMap<String, u64>, date: &str) -> u64 {
        if !self.baseline_set {
            self.baseline_set = true;
            self.ledger = today.clone();
            self.last_date = date.to_string();
            return 0;
        }
        let delta = if date != self.last_date {
            // A new day's totals aren't comparable to yesterday's: count them all.
            self.last_date = date.to_string();
            self.ledger = today.clone();
            today.values().sum()
        } else {
            let mut delta = 0;
            for (provider, &current) in today {
                match self.ledger.get(provider) {
                    // Newly seen provider: seed, don't backfill its past logs.
                    None => {}
                    // Regression (logs rotated/deleted): rebase this provider only.
                    Some(&prev) if current < prev => {}
                    Some(&prev) => delta += current - prev,
                }
                self.ledger.insert(provider.clone(), current);
            }
            delta
        };
        self.used_since_install += delta;
        delta
    }

    pub fn ready_to_hatch(&self) -> bool {
        self.active.is_none() && self.baseline_set && self.egg_usage >= EGG_HATCH_THRESHOLD
    }

    /// Feeds tokens to the egg or the active Pokémon.
    pub fn feed(&mut self, delta: u64, rng: &mut Rng) -> Vec<Event> {
        if self.active.is_none() {
            self.egg_usage += delta;
            return vec![];
        }
        self.active.as_mut().unwrap().used_at_stage += delta;
        let mut events = vec![];
        while let Some(mon) = self.active.as_mut() {
            let thr = mon.threshold();
            if mon.used_at_stage < thr {
                break;
            }
            let node = mon.line.tree.find(mon.current_id()).cloned();
            let Some(node) = node.filter(|n| !n.children.is_empty()) else {
                events.push(self.graduate());
                break;
            };
            let next_idx = mon.stage_index + 1;
            let next = match mon.planned_ids.get(next_idx) {
                Some(&p) if node.children.iter().any(|c| c.id == p) => p,
                _ => {
                    // Stale plan: re-plan from here.
                    let base = mon.line.base_id;
                    let route = plan(&node, base, &self.collected_finals, rng);
                    let mon = self.active.as_mut().unwrap();
                    mon.planned_ids.truncate(mon.stage_index);
                    mon.planned_ids.extend(route);
                    mon.planned_ids[next_idx]
                }
            };
            let mon = self.active.as_mut().unwrap();
            mon.path_ids.truncate(next_idx);
            mon.path_ids.push(next);
            mon.stage_index = next_idx;
            mon.used_at_stage -= thr; // overflow carries over
            events.push(Event::Evolved { id: next });
        }
        events
    }

    fn graduate(&mut self) -> Event {
        let mon = self.active.take().unwrap();
        let final_id = mon.current_id();
        self.collected_finals.insert(format!("{}:{}", mon.line.base_id, final_id));
        let names = mon
            .path_ids
            .iter()
            .filter_map(|id| mon.line.names.get(id).map(|n| (*id, n.clone())))
            .collect();
        self.dex.push(DexEntry {
            base_id: mon.line.base_id,
            final_id,
            chain: mon.path_ids,
            rarity: mon.line.rarity,
            is_shiny: mon.is_shiny,
            nature: mon.nature,
            names,
            caught_at: chrono::Local::now().to_rfc3339(),
        });
        self.egg_usage = 0;
        Event::Graduated { id: final_id }
    }

    pub fn hatch(&mut self, line: Line, rng: &mut Rng) -> Vec<Event> {
        let overflow = self.egg_usage.saturating_sub(EGG_HATCH_THRESHOLD);
        self.egg_usage = 0;
        let is_shiny = rng.u64(..) % SHINY_DENOMINATOR == 0;
        let nature = NATURES[rng.usize(..NATURES.len())].to_string();
        let planned_ids = plan(&line.tree, line.base_id, &self.collected_finals, rng);
        let base_id = line.base_id;
        self.active = Some(Mon {
            line,
            path_ids: vec![base_id],
            planned_ids,
            stage_index: 0,
            used_at_stage: 0,
            is_shiny,
            nature,
        });
        let mut events = vec![Event::Hatched { id: base_id, shiny: is_shiny }];
        events.extend(self.feed(overflow, rng));
        events
    }

    pub fn has_collected_base(&self, base_id: u32) -> bool {
        let prefix = format!("{base_id}:");
        self.collected_finals.iter().any(|f| f.starts_with(&prefix))
    }
}

/// Prefer a child that still leads to an uncollected final form.
fn pick_child<'a>(node: &'a EvoNode, base: u32, collected: &HashSet<String>, rng: &mut Rng) -> &'a EvoNode {
    let fresh: Vec<&EvoNode> = node
        .children
        .iter()
        .filter(|c| c.final_ids().iter().any(|f| !collected.contains(&format!("{base}:{f}"))))
        .collect();
    let pool: Vec<&EvoNode> = if fresh.is_empty() { node.children.iter().collect() } else { fresh };
    pool[rng.usize(..pool.len())]
}

/// Full route from `root` to a final form.
fn plan(root: &EvoNode, base: u32, collected: &HashSet<String>, rng: &mut Rng) -> Vec<u32> {
    let mut route = vec![root.id];
    let mut node = root;
    while !node.children.is_empty() {
        node = pick_child(node, base, collected, rng);
        route.push(node.id);
    }
    route
}

/// Capture-rate weighted pick from `(species id, capture rate)`; collected lines count half.
pub fn choose_base(index: &[(u32, u32)], state: &State, rng: &mut Rng) -> Option<u32> {
    let weights: Vec<u64> = index
        .iter()
        .map(|&(id, rate)| {
            let w = rate.max(1) as u64;
            if state.has_collected_base(id) { (w / 2).max(1) } else { w }
        })
        .collect();
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.u64(..total);
    for (i, w) in weights.iter().enumerate() {
        if r < *w {
            return Some(index[i].0);
        }
        r -= w;
    }
    None
}

/// JSON file persistence. A file that fails to parse is kept as `.corrupt` instead of overwritten.
pub struct Store {
    pub path: PathBuf,
    pub state: State,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                let _ = fs::rename(&path, path.with_extension("json.corrupt"));
                State::default()
            }),
            Err(_) => State::default(),
        };
        Store { path, state }
    }

    pub fn save(&self) {
        let tmp = self.path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_vec_pretty(&self.state) {
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.path);
            }
        }
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, children: Vec<EvoNode>) -> EvoNode {
        EvoNode { id, children }
    }

    fn bulbasaur() -> Line {
        Line {
            base_id: 1,
            tree: node(1, vec![node(2, vec![node(3, vec![])])]),
            rarity: Rarity::Common,
            names: HashMap::new(),
        }
    }

    #[test]
    fn thresholds_sum_to_graduation_total() {
        let sum: u64 = (0..3).map(|i| phase_threshold(Rarity::Common, 3, i)).sum();
        assert_eq!(sum, 750_000_000);
        assert_eq!(phase_threshold(Rarity::Common, 3, 0), 125_000_000);
    }

    #[test]
    fn rarity_from_capture_rate() {
        assert_eq!(Rarity::from(45, false), Rarity::Rare);
        assert_eq!(Rarity::from(120, false), Rarity::Uncommon);
        assert_eq!(Rarity::from(255, false), Rarity::Common);
        assert_eq!(Rarity::from(3, true), Rarity::Legendary);
    }

    #[test]
    fn credit_seeds_then_counts_growth() {
        let mut s = State::default();
        let day = |c: u64, x: u64| HashMap::from([("claude".into(), c), ("codex".into(), x)]);
        assert_eq!(s.credit(&day(100, 0), "d1"), 0); // baseline
        assert_eq!(s.credit(&day(150, 10), "d1"), 60);
        assert_eq!(s.credit(&day(120, 10), "d1"), 0); // regression rebases
        assert_eq!(s.credit(&day(130, 10), "d1"), 10);
        assert_eq!(s.credit(&day(5, 0), "d2"), 5); // new day counts in full
        assert_eq!(s.used_since_install, 75);
    }

    #[test]
    fn hatch_evolve_graduate_with_overflow() {
        let mut rng = Rng::with_seed(7);
        let mut s = State { baseline_set: true, ..Default::default() };
        s.feed(EGG_HATCH_THRESHOLD + 1_000, &mut rng);
        assert!(s.ready_to_hatch());
        let ev = s.hatch(bulbasaur(), &mut rng);
        assert!(matches!(ev[0], Event::Hatched { id: 1, .. }));
        assert_eq!(s.active.as_ref().unwrap().used_at_stage, 1_000);

        // 125M finishes stage 0; the extra 1_000 + 250M jumps straight through stage 1 too.
        let ev = s.feed(125_000_000 + 250_000_000, &mut rng);
        assert_eq!(ev.len(), 2);
        assert_eq!(s.active.as_ref().unwrap().current_id(), 3);
        assert_eq!(s.active.as_ref().unwrap().used_at_stage, 1_000);

        let ev = s.feed(375_000_000, &mut rng);
        assert!(matches!(ev[0], Event::Graduated { id: 3 }));
        assert!(s.active.is_none());
        assert_eq!(s.dex[0].chain, vec![1, 2, 3]);
        assert!(s.has_collected_base(1));
    }

    #[test]
    fn branches_prefer_uncollected_final() {
        let mut rng = Rng::with_seed(1);
        let tree = node(133, vec![node(134, vec![]), node(135, vec![])]);
        let collected = HashSet::from(["133:134".to_string()]);
        for _ in 0..20 {
            assert_eq!(plan(&tree, 133, &collected, &mut rng), vec![133, 135]);
        }
    }

    #[test]
    fn keep_animated_cuts_late_gen_evolutions() {
        let tree = node(439, vec![node(122, vec![node(866, vec![])])]);
        assert_eq!(tree.keep_animated().unwrap(), node(439, vec![node(122, vec![])]));
    }

    #[test]
    fn choose_base_halves_collected_weight() {
        let mut rng = Rng::with_seed(3);
        let mut s = State::default();
        s.collected_finals.insert("1:3".into());
        let index = [(1, 100), (4, 100)];
        let picks = (0..3000).filter(|_| choose_base(&index, &s, &mut rng) == Some(1)).count();
        assert!((800..1200).contains(&picks), "{picks}"); // ~1/3
    }
}
