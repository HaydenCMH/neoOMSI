use legacy_config::{content_changed, find_in_dir, name_key, resolve_existing_with, vfs, windows_components};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn resolve_existing(base: &Path, rel: &[&str]) -> Option<PathBuf> {
    resolve_existing_with(base, rel, &|cur| {
        !CONTENT_ROOTS.lock().unwrap().iter().any(|r| r == cur)
    })
}

fn pop_within_roots(cur: &mut PathBuf) {
    if CONTENT_ROOTS.lock().unwrap().iter().any(|r| r == cur) {
        return;
    }
    cur.pop();
}

static CONTENT_ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub fn add_content_root(root: PathBuf) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    if !r.iter().any(|x| x == &root) {
        r.push(root);
        drop(r);
        content_changed();
    }
}

pub fn add_content_root_before(root: PathBuf, before: &Path) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    if r.iter().any(|x| x == &root) {
        return;
    }
    match r.iter().position(|x| x == before) {
        Some(i) => r.insert(i, root),
        None => r.push(root),
    }
    drop(r);
    content_changed();
}

pub fn add_content_root_first(root: PathBuf) {
    let mut r = CONTENT_ROOTS.lock().unwrap();
    r.retain(|x| x != &root);
    r.insert(0, root);
    drop(r);
    content_changed();
}

pub fn remove_content_root(root: &Path) {
    CONTENT_ROOTS.lock().unwrap().retain(|x| x != root);
    content_changed();
}

static SANDBOX_ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub fn mark_sandbox(root: PathBuf) {
    let mut r = SANDBOX_ROOTS.lock().unwrap();
    if !r.contains(&root) {
        r.push(root);
    }
}

pub fn is_sandbox(path: &Path) -> bool {
    SANDBOX_ROOTS
        .lock()
        .unwrap()
        .iter()
        .any(|r| path.starts_with(r))
}

pub fn find_in_roots(rel: &str) -> Option<(PathBuf, PathBuf)> {
    let comps = windows_components(rel);
    content_roots()
        .into_iter()
        .find_map(|r| resolve_existing(&r, &comps).map(|p| (r, p)))
}

pub fn content_roots() -> Vec<PathBuf> {
    CONTENT_ROOTS.lock().unwrap().clone()
}

pub fn mirrored_dirs(dir: &Path) -> Vec<PathBuf> {
    let roots = content_roots();
    let Some(suffix) = owner_root(dir, &roots).and_then(|r| dir.strip_prefix(r).ok()) else {
        return vec![dir.to_path_buf()];
    };
    let rel: Vec<String> = suffix
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let rel: Vec<&str> = rel.iter().map(|s| s.as_str()).collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for r in &roots {
        let found = if rel.is_empty() {
            Some(r.clone())
        } else {
            resolve_existing(r, &rel)
        };
        if let Some(p) = found.filter(|p| vfs::is_dir(p)) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        out.push(dir.to_path_buf());
    }
    out
}

pub(crate) fn owner_root<'a>(path: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())
}

/// The `Vehicles/<pack>` of `path`, when no content root has it.
pub fn missing_vehicle_pack(path: &Path) -> Option<String> {
    let roots = content_roots();
    let owner = owner_root(path, &roots)?.clone();
    let suffix = path.strip_prefix(&owner).ok()?;
    let mut comps = suffix.components().filter_map(|c| match c {
        std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
        _ => None,
    });
    let first = comps.next()?;
    let pack = comps.next()?;
    if !first.eq_ignore_ascii_case("Vehicles") || comps.next().is_none() {
        return None;
    }
    let installed = roots.iter().any(|r| {
        resolve_existing(r, &["Vehicles", pack.as_str()])
            .map(|d| vfs::is_dir(&d))
            .unwrap_or(false)
    });
    (!installed).then_some(pack)
}

pub fn content_dirs(rel: &str) -> Vec<PathBuf> {
    let rel = windows_components(rel);
    content_roots()
        .iter()
        .filter_map(|r| resolve_existing(r, &rel))
        .filter(|p| vfs::is_dir(p))
        .collect()
}

pub fn read_dir_merged(rel: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for d in content_dirs(rel) {
        for (name, _) in vfs::list_dir(&d).unwrap_or_default() {
            if seen.insert(name_key(&name.to_string_lossy())) {
                out.push(d.join(name));
            }
        }
    }
    out
}

/// The roots that have `path`'s pack, so lookups skip the rest; `None` when unknown.
fn roots_with_pack(path: &[&str]) -> Option<Vec<PathBuf>> {
    type Index = (u64, HashMap<String, Vec<PathBuf>>);
    static INDEX: Mutex<Option<Index>> = Mutex::new(None);
    let ([top, pack], true) = (path.first_chunk::<2>()?, path.len() > 2) else {
        return None;
    };
    if !top.is_ascii() || !pack.is_ascii() {
        return None;
    }
    let key = format!("{}/{}", name_key(top), name_key(pack));
    let mut index = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let generation = legacy_config::content_generation();
    if index.as_ref().is_none_or(|(g, _)| *g != generation) {
        let mut packs: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for r in content_roots() {
            for (top, is_dir) in vfs::list_dir(&r).unwrap_or_default() {
                if !is_dir {
                    continue;
                }
                let top_key = name_key(&top.to_string_lossy());
                for (pack, is_dir) in vfs::list_dir(&r.join(&top)).unwrap_or_default() {
                    if is_dir {
                        let key = format!("{top_key}/{}", name_key(&pack.to_string_lossy()));
                        let roots = packs.entry(key).or_default();
                        if !roots.contains(&r) {
                            roots.push(r.clone());
                        }
                    }
                }
            }
        }
        *index = Some((generation, packs));
    }
    Some(index.as_ref()?.1.get(&key).cloned().unwrap_or_default())
}

/// The file the game uses for `rel` relative to `base`: a higher root wins, and a `..`
/// never leaves a root.
pub fn resolve_path(base: &Path, rel: &str) -> PathBuf {
    let rel = windows_components(rel);
    let rel = rel.as_slice();
    let roots = content_roots();
    let owner = owner_root(base, &roots).cloned();
    let from_top: Vec<String> = owner
        .as_ref()
        .and_then(|o| base.strip_prefix(o).ok())
        .map(|s| s.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let mut path: Vec<&str> = Vec::new();
    for c in from_top.iter().map(String::as_str).chain(rel.iter().copied()) {
        // a `..` above the top stays at the top
        if c == ".." {
            path.pop();
        } else {
            path.push(c);
        }
    }
    let from_top = path;
    let candidates = roots_with_pack(&from_top);
    let has_pack = |r: &PathBuf| candidates.as_ref().is_none_or(|c| c.contains(r));
    if let Some(owner) = &owner {
        for r in roots.iter().take_while(|r| *r != owner).filter(|r| has_pack(r)) {
            if let Some(p) = resolve_existing(r, &from_top) {
                return p;
            }
        }
    }
    let mut cur = base.to_path_buf();
    for comp in rel {
        if *comp == ".." {
            pop_within_roots(&mut cur);
            continue;
        }
        let direct = cur.join(comp);
        if vfs::exists(&direct) {
            cur = direct;
            continue;
        }
        let found = find_in_dir(&cur, comp).map(|n| cur.join(n));
        cur = found.unwrap_or(direct);
    }
    if let (Some(owner), false) = (&owner, vfs::exists(&cur)) {
        for r in roots.iter().skip_while(|r| *r != owner).skip(1).filter(|r| has_pack(r)) {
            if let Some(p) = resolve_existing(r, &from_top) {
                return p;
            }
        }
    }
    if owner.is_some() && owner_root(&cur, &roots).is_none() {
        return base.to_path_buf();
    }
    cur
}

pub fn add_content_zip(path: &Path) -> io::Result<PathBuf> {
    let mount = vfs::mount_zip(path)?;
    add_content_root(mount.clone());
    Ok(mount)
}

pub fn mount_dir_zips(dir: &Path) -> Vec<PathBuf> {
    let mut zips: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                    .map(|e| e.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => return Vec::new(),
    };
    zips.sort();
    let mut out = Vec::new();
    for p in zips {
        match add_content_zip(&p) {
            Ok(m) => out.push(m),
            Err(e) => log::warn!("content zip {}: {e}", p.display()),
        }
    }
    out
}

pub fn mount_env_zips() -> Vec<PathBuf> {
    let Some(v) = std::env::var_os("OMSI_CONTENT_ZIP") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in std::env::split_paths(&v) {
        if p.as_os_str().is_empty() {
            continue;
        }
        match add_content_zip(&p) {
            Ok(m) => out.push(m),
            Err(e) => log::warn!("content zip {}: {e}", p.display()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_like_windows() {
        let dir = std::env::temp_dir().join(format!("mods-paths-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Texture").join("Bahn\u{fc}bergang")).unwrap();
        std::fs::write(
            dir.join("Texture").join("Bahn\u{fc}bergang").join("anz-oben.jpg"),
            b"x",
        )
            .unwrap();
        let p = resolve_path(
            &dir.join("model"),
            "..\\TEXTURE.\\BAHN\u{dc}BERGANG/Anz-Oben.JPG. ",
        );
        assert!(vfs::is_file(&p), "{}", p.display());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn never_leaves_the_roots() {
        let dir = std::env::temp_dir().join(format!("mods-escape-{}", std::process::id()));
        let root = dir.join("root");
        std::fs::create_dir_all(root.join("Vehicles")).unwrap();
        std::fs::write(dir.join("secret.txt"), b"x").unwrap();
        add_content_root(root.clone());
        let p = resolve_path(&root.join("Vehicles"), "..\\..\\secret.txt");
        assert!(p.starts_with(&root), "{}", p.display());
        assert!(!vfs::exists(&resolve_path(&root, "C:\\secret.txt")));
        remove_content_root(&root);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn layers(test: &str, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let dir = std::env::temp_dir().join(format!("mods-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let roots: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
        for r in &roots {
            std::fs::create_dir_all(r).unwrap();
            add_content_root(r.clone());
        }
        (dir, roots)
    }

    fn put(root: &Path, rel: &str, data: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    fn done(dir: &Path, roots: &[PathBuf]) {
        for r in roots {
            remove_content_root(r);
        }
        std::fs::remove_dir_all(dir).ok();
    }

    fn read(p: PathBuf) -> String {
        String::from_utf8(vfs::read(&p).unwrap()).unwrap()
    }

    #[test]
    fn the_higher_root_wins_for_every_file_that_asks() {
        let (dir, r) = layers("order", &["addon", "base", "omsi"]);
        let (addon, base, omsi) = (&r[0], &r[1], &r[2]);
        put(base, "Vehicles/Bus/model/model.cfg", "base");
        put(base, "Vehicles/Bus/Texture/body.dds", "base");
        put(base, "Vehicles/Bus/Texture/seat.dds", "base");
        put(addon, "Vehicles/Bus/Texture/body.dds", "addon");
        put(omsi, "Vehicles/SD200/Texture/body.dds", "stock");
        put(addon, "Vehicles/SD200/Texture/body.dds", "addon");
        // the base mod's own reference takes the addon's file; what the addon lacks stays
        let model = base.join("Vehicles/Bus/model");
        assert_eq!(read(resolve_path(&model, "../Texture/body.dds")), "addon");
        assert_eq!(read(resolve_path(&model, "../Texture/seat.dds")), "base");
        // a stock file asks: a mod's copy replaces the original (which stays untouched)
        let stock = omsi.join("Vehicles/SD200/model");
        assert_eq!(read(resolve_path(&stock, "../Texture/body.dds")), "addon");
        assert_eq!(read(omsi.join("Vehicles/SD200/Texture/body.dds")), "stock");
        // from the top of the OMSI folder: the same
        assert_eq!(read(resolve_path(omsi, "Vehicles/Bus/Texture/body.dds")), "addon");
        done(&dir, &r);
    }

    #[test]
    fn a_mod_uses_parts_of_another_mod() {
        let (dir, r) = layers("cross", &["citaro", "urbino", "omsi"]);
        put(&r[1], "Vehicles/Urbino/model/dash.o3d", "urbino");
        std::fs::create_dir_all(r[0].join("Vehicles/Citaro/model")).unwrap();
        let p = resolve_path(&r[0].join("Vehicles/Citaro/model"), "../../Urbino/model/dash.o3d");
        assert_eq!(read(p.clone()), "urbino");
        assert_eq!(missing_vehicle_pack(&r[0].join("Vehicles/Missing/a.o3d")).as_deref(), Some("Missing"));
        assert_eq!(missing_vehicle_pack(&p), None);
        done(&dir, &r);
    }

    #[test]
    fn a_dot_dot_never_climbs_out_of_a_root() {
        let (dir, r) = layers("clamp", &["a", "b"]);
        put(&dir, "secret.txt", "x");
        put(&r[1], "Vehicles/Bus/bus.bus", "b");
        let p = resolve_path(&r[0].join("Vehicles"), "../../../secret.txt");
        assert!(r.iter().any(|root| p.starts_with(root)), "{}", p.display());
        done(&dir, &r);
    }
}
