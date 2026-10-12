//! The player's load order, top first, in `<content>/mods/.load-order.json`.

use legacy_config::name_key;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ORDER_FILE: &str = ".load-order.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewMods {
    /// A new mod wins, like a fresh copy over OMSI 2.
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrderFile {
    pub version: u32,
    /// A save made from an older revision is refused.
    pub revision: u64,
    #[serde(default)]
    pub new_mods: NewMods,
    pub order: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub available: bool,
}

pub fn load(mods_dir: &Path) -> Result<Option<OrderFile>, String> {
    let text = match std::fs::read_to_string(mods_dir.join(ORDER_FILE)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let file: OrderFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if file.version != 1 {
        return Err(format!("version {} is not known", file.version));
    }
    Ok(Some(file))
}

/// An unreadable file is kept as `.load-order.json.bad-<time>`.
pub fn save(mods_dir: &Path, file: &OrderFile) -> std::io::Result<()> {
    let path = mods_dir.join(ORDER_FILE);
    if load(mods_dir).is_err() {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::rename(&path, mods_dir.join(format!("{ORDER_FILE}.bad-{secs}")));
    }
    let tmp = mods_dir.join(format!("{ORDER_FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(file)?)?;
    std::fs::rename(&tmp, &path)
}

/// The saved order, with unlisted entries at the top or bottom.
pub fn effective(present: &[String], saved: Option<&OrderFile>) -> Vec<Item> {
    let Some(saved) = saved else {
        return present
            .iter()
            .map(|n| Item { name: n.clone(), available: true })
            .collect();
    };
    let has = |list: &[String], name: &str| list.iter().any(|n| name_key(n) == name_key(name));
    let mut listed: Vec<Item> = Vec::new();
    for name in &saved.order {
        if !listed.iter().any(|i| name_key(&i.name) == name_key(name)) {
            let on_disk = present.iter().find(|n| name_key(n) == name_key(name));
            listed.push(Item {
                name: on_disk.unwrap_or(name).clone(),
                available: on_disk.is_some(),
            });
        }
    }
    let new = present
        .iter()
        .filter(|n| !has(&saved.order, n))
        .map(|n| Item { name: n.clone(), available: true });
    match saved.new_mods {
        NewMods::Top => new.chain(listed).collect(),
        NewMods::Bottom => listed.into_iter().chain(new).collect(),
    }
}

fn write(
    content: &Path,
    change: impl FnOnce(&crate::LoadOrder) -> Result<Option<(Vec<String>, NewMods)>, String>,
) -> Result<(), String> {
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let lo = crate::load_order(content);
    let Some((order, new_mods)) = change(&lo)? else {
        return Ok(());
    };
    if lo.saved.as_ref().is_some_and(|s| s.order == order && s.new_mods == new_mods) {
        return Ok(());
    }
    let file = OrderFile {
        version: 1,
        revision: revision(&lo) + 1,
        new_mods,
        order,
    };
    save(&content.join(crate::MODS_DIR), &file).map_err(|e| e.to_string())
}

pub fn revision(lo: &crate::LoadOrder) -> u64 {
    lo.saved.as_ref().map(|s| s.revision).unwrap_or(0)
}

fn listed(lo: &crate::LoadOrder) -> Vec<String> {
    lo.items.iter().map(|i| i.name.clone()).collect()
}

fn placement(lo: &crate::LoadOrder) -> NewMods {
    lo.saved.as_ref().map(|s| s.new_mods).unwrap_or_default()
}

/// Save the order in effect, so the next new mod is placed relative to it.
pub fn materialize(content: &Path) -> Result<(), String> {
    write(content, |lo| Ok(Some((listed(lo), placement(lo)))))
}

pub fn forget(content: &Path, name: &str) -> Result<(), String> {
    write(content, |lo| {
        let mut order = listed(lo);
        order.retain(|n| name_key(n) != name_key(name));
        Ok(Some((order, placement(lo))))
    })
}

pub fn rename(content: &Path, old: &str, new: &str) -> Result<(), String> {
    write(content, |lo| {
        let mut order = listed(lo);
        order.retain(|n| name_key(n) != name_key(new));
        if let Some(i) = order.iter().position(|n| name_key(n) == name_key(old)) {
            order[i] = new.to_string();
        }
        Ok(Some((order, placement(lo))))
    })
}

pub fn set(content: &Path, order: &[String], new_mods: NewMods, read: u64) -> Result<(), String> {
    write(content, |lo| {
        if read != revision(lo) {
            return Err("the load order was changed elsewhere - it was read again".into());
        }
        let known = |n: &str| lo.items.iter().any(|i| name_key(&i.name) == name_key(n));
        let mut out: Vec<String> = Vec::new();
        for n in order {
            if known(n) && !out.iter().any(|o| name_key(o) == name_key(n)) {
                out.push(n.clone());
            }
        }
        for n in listed(lo) {
            if !out.iter().any(|o| name_key(o) == name_key(&n)) {
                out.push(n);
            }
        }
        Ok(Some((out, new_mods)))
    })
}

pub fn reset(content: &Path, read: u64) -> Result<(), String> {
    write(content, |lo| {
        if read != revision(lo) {
            return Err("the load order was changed elsewhere - it was read again".into());
        }
        Ok(Some((lo.entries.iter().map(|e| e.id.clone()).collect(), placement(lo))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[Item]) -> Vec<(&str, bool)> {
        items.iter().map(|i| (i.name.as_str(), i.available)).collect()
    }

    fn present(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn without_a_saved_order_the_content_keeps_its_order() {
        let p = present(&["Archives/A.zip", "alpha", "Beta.zip"]);
        assert_eq!(effective(&p, None).iter().map(|i| &i.name).collect::<Vec<_>>(), p.iter().collect::<Vec<_>>());
    }

    #[test]
    fn the_players_order_is_kept_and_new_mods_go_where_chosen() {
        let saved = OrderFile {
            version: 1,
            order: present(&["mod2", "Gone", "mod1"]),
            ..Default::default()
        };
        let p = present(&["mod1", "MOD2", "new_a", "new_b"]);
        assert_eq!(
            names(&effective(&p, Some(&saved))),
            [("new_a", true), ("new_b", true), ("MOD2", true), ("Gone", false), ("mod1", true)]
        );
        let bottom = OrderFile { new_mods: NewMods::Bottom, ..saved };
        assert_eq!(
            names(&effective(&p, Some(&bottom))),
            [("MOD2", true), ("Gone", false), ("mod1", true), ("new_a", true), ("new_b", true)]
        );
    }

    #[test]
    fn the_order_is_changed_one_read_at_a_time() {
        let content = std::env::temp_dir().join(format!("mods-order-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&content);
        let mods = content.join(crate::MODS_DIR);
        std::fs::create_dir_all(mods.join("A")).unwrap();
        std::fs::write(mods.join("B.zip"), b"").unwrap();
        let order = || load(&mods).unwrap().unwrap().order;
        materialize(&content).unwrap();
        assert_eq!(order(), ["A", "B.zip"]);
        let read = load(&mods).unwrap().unwrap().revision;
        set(&content, &present(&["B.zip", "A"]), NewMods::Top, read).unwrap();
        assert!(set(&content, &present(&["A"]), NewMods::Top, read).is_err(), "stale read");
        assert_eq!(order(), ["B.zip", "A"]);
        // a mod that appears goes to the top; then it is forgotten again
        std::fs::create_dir_all(mods.join("C")).unwrap();
        materialize(&content).unwrap();
        assert_eq!(order(), ["C", "B.zip", "A"]);
        forget(&content, "a").unwrap();
        assert_eq!(order(), ["C", "B.zip"]);
        reset(&content, load(&mods).unwrap().unwrap().revision).unwrap();
        assert_eq!(order(), ["A", "B.zip", "C"]);
        std::fs::remove_dir_all(&content).ok();
    }

    #[test]
    fn a_file_that_cannot_be_read_is_reported_and_kept() {
        let dir = std::env::temp_dir().join(format!("mods-order-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir), Ok(None));
        std::fs::write(dir.join(ORDER_FILE), b"{ not json").unwrap();
        assert!(load(&dir).is_err());
        let file = OrderFile { version: 1, revision: 3, order: present(&["a"]), ..Default::default() };
        save(&dir, &file).unwrap();
        assert_eq!(load(&dir), Ok(Some(file)));
        let kept = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".bad-"))
            .count();
        assert_eq!(kept, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
