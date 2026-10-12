//! How file lookups scale with the number of mods (every lookup tries the roots in priority
//! order, stock content's included): `cargo run --release -p mods --example resolve_bench`.

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
    for i in 0..STOCK_OBJECTS {
        put(&omsi.join(format!("Sceneryobjects/Stock{i}/obj.sco")));
        put(&omsi.join(format!("Sceneryobjects/Stock{i}/texture/a.dds")));
    }
    println!("{STOCK_OBJECTS} stock objects; per lookup, cold (first) and warm (cached listings):");
    for mods in [0usize, 50, 200] {
        let roots: Vec<PathBuf> = (0..mods).map(|k| base.join(format!("mod{mods}-{k}"))).collect();
        for (k, r) in roots.iter().enumerate() {
            put(&r.join(format!("Sceneryobjects/Mod{k}/obj.sco")));
            put(&r.join(format!("Vehicles/Bus{k}/model/model.cfg")));
            // every tenth mod replaces a stock texture
            if k % 10 == 0 {
                put(&r.join(format!("Sceneryobjects/Stock{k}/texture/a.dds")));
            }
            mods::add_content_root(r.clone());
        }
        mods::add_content_root(omsi.clone());
        let lookups = || {
            let t0 = Instant::now();
            for i in 0..STOCK_OBJECTS {
                // a map tile names a stock object; the object names its texture
                let sco = mods::resolve_path(&omsi, &format!("Sceneryobjects\\Stock{i}\\obj.sco"));
                let dir = sco.parent().unwrap().to_path_buf();
                let _ = mods::resolve_path(&dir, "texture\\a.dds");
            }
            t0.elapsed().as_secs_f64() * 1e6 / (2 * STOCK_OBJECTS) as f64
        };
        let (cold, warm) = (lookups(), lookups());
        println!("  {mods:>3} mods: {cold:>7.1} us cold, {warm:>6.1} us warm");
        for r in roots.iter().chain([&omsi]) {
            mods::remove_content_root(r);
        }
    }
    std::fs::remove_dir_all(&base).ok();
}
