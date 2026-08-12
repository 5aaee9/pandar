use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

pub(super) type PresetMap = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Default)]
struct Cache {
    generation: Option<(u64, u64)>,
    presets: PresetMap,
}

fn caches() -> &'static Mutex<BTreeMap<u64, Cache>> {
    static CACHES: OnceLock<Mutex<BTreeMap<u64, Cache>>> = OnceLock::new();
    CACHES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn reset(identity: u64) {
    caches()
        .lock()
        .expect("personal preset cache")
        .remove(&identity);
}

pub(super) fn publish(identity: u64, generation: (u64, u64), presets: PresetMap) {
    caches().lock().expect("personal preset cache").insert(
        identity,
        Cache {
            generation: Some(generation),
            presets,
        },
    );
}

pub(super) fn drain(identity: u64, generation: (u64, u64)) -> PresetMap {
    let mut caches = caches().lock().expect("personal preset cache");
    let Some(cache) = caches.get_mut(&identity) else {
        return PresetMap::new();
    };
    if cache.generation != Some(generation) {
        caches.remove(&identity);
        return PresetMap::new();
    }
    cache.generation = None;
    std::mem::take(&mut cache.presets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_is_agent_and_generation_scoped_and_drains_once() {
        reset(1);
        reset(2);
        publish(
            1,
            (1, 2),
            BTreeMap::from([("Fine".into(), BTreeMap::new())]),
        );
        publish(
            2,
            (1, 2),
            BTreeMap::from([("Other".into(), BTreeMap::new())]),
        );
        assert!(drain(1, (9, 9)).is_empty());
        assert_eq!(drain(2, (1, 2)).len(), 1);
        publish(
            1,
            (1, 2),
            BTreeMap::from([("Fine".into(), BTreeMap::new())]),
        );
        assert_eq!(drain(1, (1, 2)).len(), 1);
        assert!(drain(1, (1, 2)).is_empty());
    }
}
