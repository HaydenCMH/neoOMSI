//! Which mounted entry wins which files (display only).

use legacy_config::vfs;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conflict {
    pub id: String,
    pub files: usize,
    /// Files that hide an entry below.
    pub overrides: usize,
    /// Files hidden by an entry above.
    pub overridden: usize,
    /// Files that replace an original OMSI 2 file.
    pub replaces_original: usize,
    pub hidden_by: Vec<String>,
    pub overrides_mods: Vec<String>,
    /// `Vehicles/<name>` folders another entry fills with different vehicles.
    pub clashes: Vec<String>,
}

pub fn conflicts() -> Vec<Conflict> {
    let (roots, legacy) = {
        let st = crate::STATE.read().unwrap();
        let Some(st) = st.as_ref() else {
            return Vec::new();
        };
        (st.roots.clone(), st.legacy.clone())
    };
    let files: Vec<Vec<String>> = roots.iter().map(|(_, r)| files_of(r)).collect();
    let mut out: Vec<Conflict> = roots
        .iter()
        .zip(&files)
        .map(|((m, _), f)| Conflict {
            id: m.id.clone(),
            files: f.len(),
            ..Default::default()
        })
        .collect();
    let mut winner: HashMap<&str, usize> = HashMap::new();
    for (i, list) in files.iter().enumerate() {
        for f in list {
            match winner.get(f.as_str()) {
                Some(&w) => {
                    out[i].overridden += 1;
                    let (above, below) = (out[w].id.clone(), out[i].id.clone());
                    add(&mut out[i].hidden_by, above);
                    out[w].overrides += 1;
                    add(&mut out[w].overrides_mods, below);
                }
                None => {
                    winner.insert(f, i);
                }
            }
        }
    }
    if let Some(l) = &legacy {
        for (f, &w) in &winner {
            let comps: Vec<&str> = f.split('/').collect();
            if legacy_config::resolve_existing(l, &comps).is_some() {
                out[w].replaces_original += 1;
            }
        }
    }
    let vehicles: Vec<HashMap<&str, Vec<&str>>> = files.iter().map(|f| vehicle_definitions(f)).collect();
    for i in 0..out.len() {
        for j in 0..out.len() {
            for (pack, defs) in &vehicles[i] {
                if i != j && vehicles[j].get(pack).is_some_and(|other| other != defs) {
                    let clash = format!("{pack} ({})", out[j].id);
                    add(&mut out[i].clashes, clash);
                }
            }
        }
    }
    out
}

fn add(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

fn files_of(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        for (name, is_dir) in vfs::list_dir(dir).unwrap_or_default() {
            let lower = name.to_string_lossy().to_lowercase();
            let path = if rel.is_empty() { lower } else { format!("{rel}/{lower}") };
            if is_dir {
                walk(&dir.join(&name), &path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// `vehicles/<folder>` -> its `.bus`/`.ovh`/`.zug` files.
fn vehicle_definitions(files: &[String]) -> HashMap<&str, Vec<&str>> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for f in files {
        let parts: Vec<&str> = f.splitn(3, '/').collect();
        if let [first, _, file] = parts[..] {
            if first == "vehicles"
                && !file.contains('/')
                && [".bus", ".ovh", ".zug"].iter().any(|x| file.ends_with(x))
            {
                let pack = &f[..f.len() - file.len() - 1];
                out.entry(pack).or_default().push(file);
            }
        }
    }
    for defs in out.values_mut() {
        defs.sort_unstable();
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn vehicle_folders_are_told_apart_by_their_vehicles() {
        let files: Vec<String> = ["vehicles/lion/lion.bus", "vehicles/lion/model/m.cfg", "maps/x/global.cfg"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let defs = super::vehicle_definitions(&files);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs["vehicles/lion"], ["lion.bus"]);
    }
}
