//! Files a loader needed and did not find, grouped by likely cause.

use crate::{content_roots, folder_content_root, owner_root, MODS_DIR};
use legacy_config::{resolve_existing, vfs};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Tile,
    Object,
    Spline,
    Model,
    Mesh,
    Texture,
    Script,
    Sound,
    /// A vehicle a map's AI lists name.
    Vehicle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Optional,
    Degraded,
    Required,
}

impl Kind {
    fn severity(self) -> Severity {
        match self {
            Kind::Sound => Severity::Optional,
            Kind::Mesh | Kind::Texture => Severity::Degraded,
            Kind::Tile
            | Kind::Object
            | Kind::Spline
            | Kind::Model
            | Kind::Script
            | Kind::Vehicle => {
                Severity::Required
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    MissingFile,
    IncompleteMod,
    IncompletePack,
    InactiveDependency,
    MissingDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub class: Class,
    /// `OMSI 2` for the original installation.
    pub requester: String,
    pub pack: Option<String>,
    pub provider: Option<String>,
    pub severity: Severity,
    pub count: usize,
    pub samples: Vec<String>,
}

struct Miss {
    requester: String,
    requester_root: Option<PathBuf>,
    path: String,
    kind: Kind,
}

static MISSES: Mutex<Vec<Miss>> = Mutex::new(Vec::new());
const MAX_MISSES: usize = 5000;
const SAMPLES: usize = 5;

fn miss(requester: &Path, wanted: &Path, kind: Kind, roots: &[PathBuf]) -> Miss {
    let requester_root = owner_root(requester, roots).cloned();
    let path = match owner_root(wanted, roots) {
        Some(r) => content_path(wanted, r),
        None => wanted.display().to_string(),
    };
    Miss {
        requester: entry_name(requester_root.as_deref()),
        requester_root,
        path,
        kind,
    }
}

/// Record that `wanted`, needed by `requester`, could not be loaded.
pub fn missing(requester: &Path, wanted: &Path, kind: Kind) {
    if vfs::exists(wanted) {
        return; // damaged, not missing
    }
    let Miss { requester: entry, requester_root, path, kind } =
        miss(requester, wanted, kind, &content_roots());
    let mut misses = MISSES.lock().unwrap_or_else(|e| e.into_inner());
    if misses.len() >= MAX_MISSES
        || misses
            .iter()
            .any(|m| m.requester == entry && m.path.eq_ignore_ascii_case(&path))
    {
        return;
    }
    log::warn!("{entry}: {path} not found");
    misses.push(Miss { requester: entry, requester_root, path, kind });
}

pub fn findings() -> Vec<Finding> {
    group(&MISSES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Like [`missing`] for many files at once, without recording them.
pub fn check(requester: &Path, wanted: &[(PathBuf, Kind)]) -> Vec<Finding> {
    let roots = content_roots();
    let misses: Vec<Miss> = wanted
        .iter()
        .filter(|(p, _)| !vfs::exists(p))
        .map(|(p, kind)| miss(requester, p, *kind, &roots))
        .collect();
    group(&misses)
}

fn group(misses: &[Miss]) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for m in misses.iter() {
        let pack = pack_of(&m.path);
        let (class, provider) = classify(m.requester_root.as_deref(), pack.as_deref());
        let group = out.iter_mut().find(|f| {
            f.requester == m.requester
                && f.class == class
                && match &pack {
                    Some(p) => f.pack.as_deref().is_some_and(|q| q.eq_ignore_ascii_case(p)),
                    None => f.pack.is_none(),
                }
        });
        let f = match group {
            Some(f) => f,
            None => {
                out.push(Finding {
                    class,
                    requester: m.requester.clone(),
                    pack: pack.clone(),
                    provider,
                    severity: m.kind.severity(),
                    count: 0,
                    samples: Vec::new(),
                });
                out.last_mut().unwrap()
            }
        };
        f.count += 1;
        f.severity = f.severity.max(m.kind.severity());
        if f.samples.len() < SAMPLES {
            f.samples.push(m.path.clone());
        }
    }
    out
}

/// One line for the player; `None` when only optional files are missing.
pub fn summary(findings: &[Finding]) -> Option<String> {
    let findings: Vec<&Finding> =
        findings.iter().filter(|f| f.severity > Severity::Optional).collect();
    let packs: Vec<&str> = findings
        .iter()
        .filter(|f| matches!(f.class, Class::MissingDependency | Class::InactiveDependency))
        .filter_map(|f| f.pack.as_deref())
        .collect();
    let incomplete = findings
        .iter()
        .filter(|f| matches!(f.class, Class::IncompleteMod | Class::IncompletePack))
        .count();
    let files = findings.iter().filter(|f| f.class == Class::MissingFile).count();
    let mut parts = Vec::new();
    if !packs.is_empty() {
        let mut shown = packs[..packs.len().min(3)].join(", ");
        if packs.len() > 3 {
            shown.push_str(", ...");
        }
        parts.push(format!("needs packs that are not in use: {shown}"));
    }
    if incomplete > 0 {
        parts.push(format!("{incomplete} incomplete mod(s) or pack(s)"));
    }
    if files > 0 {
        parts.push(format!("files missing in {files} place(s)"));
    }
    (!parts.is_empty()).then(|| {
        format!("Content {} - see the session in the launcher", parts.join("; "))
    })
}

impl Finding {
    /// What a dismissal matches; a finding that changes class shows again.
    pub fn key(&self) -> String {
        let what = self.pack.clone().or_else(|| self.samples.first().cloned()).unwrap_or_default();
        format!("{}|{}|{what}", self.requester, serde_json::to_string(&self.class).unwrap_or_default())
    }
}

pub fn dismissed(path: &Path) -> Vec<String> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn dismiss(path: &Path, key: &str) -> std::io::Result<()> {
    let mut keys = dismissed(path);
    if !keys.iter().any(|k| k == key) {
        keys.push(key.to_string());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&keys)?)?;
    std::fs::rename(&tmp, path)
}

pub fn write(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&findings())?)?;
    std::fs::rename(&tmp, path)
}

fn content_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

fn entry_name(root: Option<&Path>) -> String {
    let Some(root) = root else {
        return "?".into();
    };
    let st = crate::STATE.read().unwrap();
    if let Some(st) = st.as_ref() {
        if let Some((m, _)) = st.roots.iter().find(|(_, r)| r == root) {
            return m.id.clone();
        }
        if st.legacy.as_deref() == Some(root) {
            return "OMSI 2".into();
        }
    }
    root.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn pack_of(path: &str) -> Option<String> {
    const PACKS: [&str; 6] = ["Vehicles", "Sceneryobjects", "Splines", "Humans", "Trains", "maps"];
    let mut parts = path.split('/');
    let (first, pack) = (parts.next()?, parts.next()?);
    parts.next()?;
    let folder = PACKS.iter().find(|p| p.eq_ignore_ascii_case(first))?;
    Some(format!("{folder}/{pack}"))
}

fn classify(requester_root: Option<&Path>, pack: Option<&str>) -> (Class, Option<String>) {
    let Some(pack) = pack else {
        return (Class::MissingFile, None);
    };
    let comps: Vec<&str> = pack.split('/').collect();
    let has = |root: &Path| resolve_existing(root, &comps).is_some_and(|d| vfs::is_dir(&d));
    if requester_root.is_some_and(has) {
        return (Class::IncompleteMod, None);
    }
    if let Some(r) = content_roots().into_iter().find(|r| has(r)) {
        return (Class::IncompletePack, Some(entry_name(Some(&r))));
    }
    if let Some(place) = uninstalled_with(&comps) {
        return (Class::InactiveDependency, Some(place));
    }
    (Class::MissingDependency, None)
}

fn uninstalled_with(pack: &[&str]) -> Option<String> {
    let dir = crate::mods_dir()?.join(crate::UNINSTALLED_DIR);
    let rd = std::fs::read_dir(&dir).ok()?;
    rd.flatten().map(|e| e.path()).find_map(|p| {
        let found = if p.is_dir() {
            resolve_existing(&folder_content_root(&p), pack).is_some()
        } else {
            vfs::ZipArchive::open(&p).is_ok_and(|z| z.contains(&pack.join("/")))
        };
        found.then(|| format!("{}/uninstalled/{}", MODS_DIR, p.file_name().unwrap_or_default().to_string_lossy()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_in_a_pack_names_the_pack() {
        assert_eq!(pack_of("Sceneryobjects/Signs/a.sco").as_deref(), Some("Sceneryobjects/Signs"));
        assert_eq!(pack_of("vehicles/Bus/model/m.o3d").as_deref(), Some("Vehicles/Bus"));
        assert_eq!(pack_of("Texture/road.dds"), None);
        assert_eq!(pack_of("Sceneryobjects/a.sco"), None);
    }

    #[test]
    fn a_look_before_the_start_reports_only_what_is_not_there() {
        let dir = std::env::temp_dir().join(format!("mods-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (map_mod, omsi) = (dir.join("map"), dir.join("omsi"));
        std::fs::create_dir_all(map_mod.join("maps/Town")).unwrap();
        std::fs::create_dir_all(omsi.join("Sceneryobjects/Stock")).unwrap();
        std::fs::write(omsi.join("Sceneryobjects/Stock/a.sco"), b"").unwrap();
        for r in [&map_mod, &omsi] {
            crate::add_content_root(r.clone());
        }
        let found = check(
            &map_mod.join("maps/Town"),
            &[
                (omsi.join("Sceneryobjects/Stock/a.sco"), Kind::Object),
                (omsi.join("Vehicles/OnlyInThisCheck/x.bus"), Kind::Vehicle),
            ],
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].class.clone(), found[0].pack.as_deref()), (Class::MissingDependency, Some("Vehicles/OnlyInThisCheck")));
        for r in [&map_mod, &omsi] {
            crate::remove_content_root(r);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_silent_sound_alone_is_not_worth_a_message() {
        let finding = |severity| Finding {
            class: Class::MissingDependency,
            requester: "Bus".into(),
            pack: Some("Vehicles/Sounds".into()),
            provider: None,
            severity,
            count: 1,
            samples: Vec::new(),
        };
        assert_eq!(summary(&[finding(Severity::Optional)]), None);
        assert!(summary(&[finding(Severity::Degraded)]).is_some());
    }

    #[test]
    fn misses_are_told_apart_by_where_the_pack_is() {
        let dir = std::env::temp_dir().join(format!("mods-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (map_mod, signs, omsi) = (dir.join("map"), dir.join("signs"), dir.join("omsi"));
        std::fs::create_dir_all(map_mod.join("maps/Town")).unwrap();
        std::fs::create_dir_all(map_mod.join("Sceneryobjects/TownObjects")).unwrap();
        std::fs::create_dir_all(signs.join("Sceneryobjects/Signs")).unwrap();
        std::fs::create_dir_all(&omsi).unwrap();
        for r in [&map_mod, &signs, &omsi] {
            crate::add_content_root(r.clone());
        }
        let map = map_mod.join("maps/Town");
        let at = |rel: &str| omsi.join(rel);
        let (own, other, nowhere) = (
            classify(Some(&map_mod), pack_of("Sceneryobjects/TownObjects/x.sco").as_deref()),
            classify(Some(&map_mod), pack_of("Sceneryobjects/Signs/y.sco").as_deref()),
            classify(Some(&map_mod), pack_of("Splines/Hamburg/z.sli").as_deref()),
        );
        assert_eq!(own.0, Class::IncompleteMod);
        assert_eq!(other.0, Class::IncompletePack);
        assert_eq!(nowhere, (Class::MissingDependency, None));
        assert_eq!(classify(None, pack_of("Texture/a.dds").as_deref()).0, Class::MissingFile);
        // recorded once per asking entry and file; present files are no misses
        missing(&map, &at("Splines/Hamburg/z.sli"), Kind::Spline);
        missing(&map, &at("Splines/Hamburg/z.sli"), Kind::Spline);
        missing(&map, &at("Splines/Hamburg/w.sli"), Kind::Spline);
        missing(&map, &signs.join("Sceneryobjects/Signs"), Kind::Object);
        let f: Vec<Finding> = findings()
            .into_iter()
            .filter(|f| f.pack.as_deref() == Some("Splines/Hamburg"))
            .collect();
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].count, f[0].severity), (2, Severity::Required));
        assert!(summary(&f).unwrap().contains("Splines/Hamburg"));
        for r in [&map_mod, &signs, &omsi] {
            crate::remove_content_root(r);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
