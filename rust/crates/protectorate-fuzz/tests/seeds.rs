//! Replays every committed corpus file through its fuzz target, so the stable
//! test suite keeps each seed and fixed crash passing.
#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

static CURRENT: Mutex<String> = Mutex::new(String::new());

#[test]
fn every_corpus_seed_passes_its_target() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/corpus");
    // A panic on a driver or bridge thread must fail the run, as under libFuzzer.
    std::panic::set_hook(Box::new(|info| {
        let seed = CURRENT.lock().map(|s| s.clone()).unwrap_or_default();
        eprintln!("seed {seed}: {info}");
        std::process::abort();
    }));
    let mut dirs: Vec<String> = fs::read_dir(&corpus)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    let names: Vec<&str> = protectorate_fuzz::TARGETS.iter().map(|(n, _)| *n).collect();
    assert_eq!(dirs, names, "corpus directories and targets differ");
    for (name, target) in protectorate_fuzz::TARGETS {
        let mut seeds: Vec<PathBuf> = fs::read_dir(corpus.join(name))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        seeds.sort();
        assert!(!seeds.is_empty(), "{name} has no seeds");
        for seed in seeds {
            *CURRENT.lock().unwrap() = seed.display().to_string();
            target(&fs::read(&seed).unwrap());
        }
    }
}
