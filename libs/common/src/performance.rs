//! Opt-in aggregate timings for local profiling; no source text is recorded.
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
    time::Instant,
};

static ENABLED: LazyLock<bool> = LazyLock::new(|| std::env::var_os("HEMTT_PROFILE").is_some());
static TIMINGS: LazyLock<Mutex<BTreeMap<&'static str, (u64, u128)>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub struct Timer {
    name: &'static str,
    start: Instant,
}
impl Timer {
    pub fn start(name: &'static str) -> Option<Self> {
        ENABLED.then(|| Self {
            name,
            start: Instant::now(),
        })
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos();
        let mut timings = TIMINGS.lock().expect("timings lock");
        let entry = timings.entry(self.name).or_default();
        entry.0 += 1;
        entry.1 += elapsed;
    }
}
pub fn snapshot() -> serde_json::Value {
    let timings = TIMINGS.lock().expect("timings lock");
    serde_json::json!({ "enabled": *ENABLED, "phases": timings.iter().map(|(name, (count, ns))| (*name, serde_json::json!({"count": count, "totalMs": *ns as f64 / 1_000_000.0}))).collect::<BTreeMap<_, _>>() })
}
