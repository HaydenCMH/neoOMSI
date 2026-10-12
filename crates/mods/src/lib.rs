use legacy_config::vfs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::SystemTime;

mod conflicts;
pub mod diagnostics;
mod layout;
pub mod order;
mod resolver;
pub use conflicts::{Conflict, conflicts};
pub use layout::{
    ARCHIVES_DIR, MODS_DIR, ModEntry, ModKind, UNINSTALLED_DIR, discover, folder_content_root,
    is_reserved,
};
pub use order::{NewMods, OrderFile};
pub use resolver::*;

struct State {
    content: PathBuf,
    legacy: Option<PathBuf>,
    mods: Vec<ModEntry>,
    /// To mount a replaced .zip anew.
    stamps: Vec<Option<(u64, SystemTime)>>,
    failed: Vec<(PathBuf, String)>,
    roots: Vec<(ModEntry, PathBuf)>,
}

static STATE: RwLock<Option<State>> = RwLock::new(None);
static REGISTERED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let md = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((md.len(), md.modified().ok()?))
}

fn is_current(
    state: &State,
    content: &Path,
    legacy: &Option<PathBuf>,
    mods: &[ModEntry],
    stamps: &[Option<(u64, SystemTime)>],
) -> bool {
    state.content == content && state.legacy == *legacy && state.mods == mods && state.stamps == stamps
}

fn archives(content: &Path) -> Vec<ModEntry> {
    let mut out: Vec<ModEntry> = std::fs::read_dir(content.join(ARCHIVES_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")))
        .map(|path| {
            let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            ModEntry {
                id: format!("{ARCHIVES_DIR}/{file}"),
                name: path.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                path,
                kind: ModKind::Zip,
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Every orderable entry, in the default order.
pub fn entries(content: &Path) -> Vec<ModEntry> {
    let mut out = archives(content);
    out.extend(discover(&content.join(MODS_DIR)));
    out
}

pub struct LoadOrder {
    /// Highest priority first.
    pub items: Vec<order::Item>,
    pub entries: Vec<ModEntry>,
    pub saved: Option<OrderFile>,
    pub error: Option<String>,
}

impl LoadOrder {
    pub fn entry(&self, id: &str) -> Option<&ModEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn active(&self) -> Vec<ModEntry> {
        self.items
            .iter()
            .filter(|i| i.available)
            .filter_map(|i| self.entry(&i.name).cloned())
            .collect()
    }
}

pub fn load_order(content: &Path) -> LoadOrder {
    let entries = entries(content);
    let (saved, error) = match order::load(&content.join(MODS_DIR)) {
        Ok(saved) => (saved, None),
        Err(e) => (None, Some(e)),
    };
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let items = order::effective(&ids, saved.as_ref());
    LoadOrder { items, entries, saved, error }
}

/// Register the mods of `content` in load order, before the OMSI folder `legacy`.
pub fn init(content: &Path, legacy: Option<&Path>) -> std::io::Result<()> {
    // the launcher's requests and its content poll call this concurrently
    static ONE: Mutex<()> = Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::create_dir_all(content.join(MODS_DIR))?;
    let legacy = legacy.map(Path::to_path_buf);
    let order = load_order(content);
    let all = order.active();
    let stamps: Vec<_> = all.iter().map(|m| stamp(&m.path)).collect();

    // re-registering clears every cache
    if STATE.read().unwrap().as_ref().is_some_and(|st| {
        is_current(st, content, &legacy, &all, &stamps)
            && !st.failed.iter().any(|(p, _)| vfs::mount_zip(p).is_ok())
    }) {
        return Ok(());
    }
    if let Some(e) = &order.error {
        log::warn!("load order of {}: {e} - the default order is used", content.display());
    }

    for r in REGISTERED.lock().unwrap().drain(..) {
        remove_content_root(&r);
        if !all.iter().any(|m| m.path == r) {
            vfs::unmount_zip(&r);
        }
    }

    let mut registered = Vec::new();
    let mut roots = Vec::new();
    let mut failed = Vec::new();
    for m in &all {
        let root = match m.kind {
            ModKind::Folder => folder_content_root(&m.path),
            ModKind::Zip => match vfs::mount_zip(&m.path) {
                Ok(mount) => mount,
                Err(e) => {
                    log::warn!("mod {}: {e}", m.path.display());
                    failed.push((m.path.clone(), e.to_string()));
                    continue;
                }
            },
        };
        match &legacy {
            Some(l) => add_content_root_before(root.clone(), l),
            None => add_content_root(root.clone()),
        }
        log::info!("mod '{}' ({:?}): {}", m.id, m.kind, m.path.display());
        registered.push(root.clone());
        roots.push((m.clone(), root));
    }
    if let Some(l) = &legacy {
        add_content_root(l.clone());
    }
    *REGISTERED.lock().unwrap() = registered;
    *STATE.write().unwrap() = Some(State {
        content: content.to_path_buf(),
        legacy,
        mods: all,
        stamps,
        failed,
        roots,
    });
    Ok(())
}

pub fn mount_error(path: &Path) -> Option<String> {
    let st = STATE.read().unwrap();
    let (_, e) = st.as_ref()?.failed.iter().find(|(p, _)| p == path)?;
    Some(e.clone())
}

pub fn mods_dir() -> Option<PathBuf> {
    STATE.read().unwrap().as_ref().map(|s| s.content.join(MODS_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_is_registered_anew_only_when_it_changed() {
        let content = PathBuf::from("content");
        let mods = vec![ModEntry {
            id: "Bus.zip".into(),
            name: "Bus".into(),
            path: content.join("mods/Bus.zip"),
            kind: ModKind::Zip,
        }];
        let then = Some((10, SystemTime::UNIX_EPOCH));
        let state = State {
            content: content.clone(),
            legacy: None,
            mods: mods.clone(),
            stamps: vec![then],
            failed: Vec::new(),
            roots: Vec::new(),
        };
        assert!(is_current(&state, &content, &None, &mods, &[then]));
        // the .zip was replaced: it is mounted anew
        let now = Some((11, SystemTime::UNIX_EPOCH));
        assert!(!is_current(&state, &content, &None, &mods, &[now]));
        assert!(!is_current(&state, &content, &None, &[], &[]));
    }

    /// The example of the improvement plan (§5.8): three sign packs, four orders.
    #[test]
    fn the_load_order_decides_which_file_the_game_uses() {
        let content = std::env::temp_dir().join(format!("mods-signs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&content);
        let mods = content.join(MODS_DIR);
        let put = |entry: &str, rel: &str, data: &str| {
            let p = mods.join(entry).join("Sceneryobjects/BVG").join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, data).unwrap();
        };
        for (pack, year) in [("BVG_Signs_2015", "2015"), ("BVG_Signs_2023", "2023")] {
            put(pack, "Haltestelle.sco", year);
            put(pack, "model/hst.o3d", year);
            put(pack, "texture/schild.dds", year);
        }
        put("HD_Signs_Patch", "texture/schild.dds", "HD");
        let omsi = content.join("omsi");
        std::fs::create_dir_all(&omsi).unwrap();
        let read = |p: PathBuf| String::from_utf8(vfs::read(&p).unwrap()).unwrap();
        for (order, expected) in [
            (["BVG_Signs_2015", "BVG_Signs_2023", "HD_Signs_Patch"], ["2015", "2015", "2015"]),
            (["HD_Signs_Patch", "BVG_Signs_2023", "BVG_Signs_2015"], ["2023", "2023", "HD"]),
            (["BVG_Signs_2015", "HD_Signs_Patch", "BVG_Signs_2023"], ["2015", "2015", "2015"]),
            (["HD_Signs_Patch", "BVG_Signs_2015", "BVG_Signs_2023"], ["2015", "2015", "HD"]),
        ] {
            let file = OrderFile {
                version: 1,
                order: order.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            };
            order::save(&mods, &file).unwrap();
            let roots: Vec<PathBuf> = load_order(&content)
                .active()
                .iter()
                .map(|m| folder_content_root(&m.path))
                .chain([omsi.clone()])
                .collect();
            for r in &roots {
                add_content_root(r.clone());
            }
            // the map names the object from the top of the OMSI folder; the object names its parts
            let sco = resolve_path(&omsi, "Sceneryobjects\\BVG\\Haltestelle.sco");
            let dir = sco.parent().unwrap().to_path_buf();
            let used = [
                read(sco),
                read(resolve_path(&dir, "model\\hst.o3d")),
                read(resolve_path(&dir, "texture\\schild.dds")),
            ];
            assert_eq!(used, expected, "order {order:?}");
            for r in &roots {
                remove_content_root(r);
            }
        }
        std::fs::remove_dir_all(&content).ok();
    }

    #[test]
    fn a_broken_archive_is_remembered_not_mounted() {
        let content = std::env::temp_dir().join(format!("mods-init-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&content);
        std::fs::create_dir_all(content.join("mods/Bus/Vehicles")).unwrap();
        std::fs::create_dir_all(content.join(ARCHIVES_DIR)).unwrap();
        std::fs::write(content.join(ARCHIVES_DIR).join("old.zip"), b"not a zip").unwrap();
        init(&content, None).unwrap();
        {
            let st = STATE.read().unwrap();
            let st = st.as_ref().unwrap();
            assert_eq!(st.failed.len(), 1);
            assert_eq!(st.failed[0].0, content.join(ARCHIVES_DIR).join("old.zip"));
            assert_eq!(st.mods.len(), 2, "the archive (in the order, not mounted) and Bus");
            assert_eq!(st.roots.len(), 1);
        }
        assert!(content_roots().contains(&content.join("mods/Bus")));
        std::fs::remove_dir_all(&content).ok();
    }
}
