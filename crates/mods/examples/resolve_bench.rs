//! Measures how file lookups scale with the number of mods, since every lookup tries the roots
//! in priority order. Run with `cargo run --release -p mods --example resolve_bench`.

use std::path::{Path, PathBuf};
use std::time::Instant;

const STOCK_OBJECTS: usize = 2000;

fn put(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

fn main() {
    let base = std::env::temp_dir().join(format!("mods-bench-{}", std::process::id()));
    let omsi = base.join("omsi");
    for object in 0..STOCK_OBJECTS {
        put(&omsi.join(format!("Sceneryobjects/Stock{object}/obj.sco")));
        put(&omsi.join(format!("Sceneryobjects/Stock{object}/texture/a.dds")));
    }
    println!("{STOCK_OBJECTS} stock objects; per lookup, cold (first) and warm (cached listings):");
    for mods in [0usize, 50, 200] {
        let roots: Vec<PathBuf> = (0..mods)
            .map(|index| base.join(format!("mod{mods}-{index}")))
            .collect();
        for (index, root) in roots.iter().enumerate() {
            put(&root.join(format!("Sceneryobjects/Mod{index}/obj.sco")));
            put(&root.join(format!("Vehicles/Bus{index}/model/model.cfg")));
            // Every tenth mod replaces a stock texture.
            if index % 10 == 0 {
                put(&root.join(format!("Sceneryobjects/Stock{index}/texture/a.dds")));
            }
            mods::add_content_root(root.clone());
        }
        mods::add_content_root(omsi.clone());
        let lookups = || {
            let started = Instant::now();
            for object in 0..STOCK_OBJECTS {
                // A map tile names a stock object, and the object names its texture.
                let sco =
                    mods::resolve_path(&omsi, &format!("Sceneryobjects\\Stock{object}\\obj.sco"));
                let dir = sco.parent().unwrap().to_path_buf();
                let _ = mods::resolve_path(&dir, "texture\\a.dds");
            }
            started.elapsed().as_secs_f64() * 1e6 / (2 * STOCK_OBJECTS) as f64
        };
        let (cold, warm) = (lookups(), lookups());
        println!("  {mods:>3} mods: {cold:>7.1} us cold, {warm:>6.1} us warm");
        for root in roots.iter().chain([&omsi]) {
            mods::remove_content_root(root);
        }
    }
    std::fs::remove_dir_all(&base).ok();
}
