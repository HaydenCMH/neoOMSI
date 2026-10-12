//! Drives `neoomsi --control-protocol` the way the launcher does, end to end, with its own home,
//! content and OMSI 2 folders, so nothing on the machine is read or written.

use launcher_protocol::api::{
    AllowModPluginsArgs, DiagnosticClass, DiagnosticsArgs, DismissDiagnosticArgs, Empty,
    ForgetModArgs, Frame, Handshake, InstallArgs, InstallMode, InstallState, MapPictureArgs,
    ModsStatus, NewMods, PreflightArgs, Request, ResetModOrderArgs, SaveModOrderArgs, StatusCode,
    UninstallModArgs, frame::Body, request::Command, response::Answer,
};
use launcher_protocol::{read_frame, write_frame};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

struct Engine {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next: u32,
}

impl Engine {
    fn start(dir: &Path) -> Engine {
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_neoomsi"))
            .arg("--control-protocol")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("APPDATA", &home)
            .env("XDG_CONFIG_HOME", &home)
            .env("OMSI_CONTENT", dir.join("content"))
            .env("OMSI_BACKGROUND", "1")
            .env("OMSI_ROOT", dir.join("omsi"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the engine starts");
        let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        Engine {
            child,
            stdin,
            stdout,
            next: 0,
        }
    }

    /// Sends `command` and returns its answer, skipping any events that arrive first.
    fn ask(&mut self, command: Command) -> Result<Answer, String> {
        self.next += 1;
        let id = format!("t{}", self.next);
        let request = Frame {
            request_id: id.clone(),
            body: Some(Body::Request(Request {
                command: Some(command),
            })),
            ..Default::default()
        };
        write_frame(&mut self.stdin, &request).unwrap();
        loop {
            let frame: Frame = read_frame(&mut self.stdout)
                .unwrap()
                .expect("the engine answers");
            if frame.request_id != id {
                continue;
            }
            if !frame.error.is_empty() {
                return Err(frame.error);
            }
            match frame.body {
                Some(Body::Response(response)) => {
                    return response.answer.ok_or_else(|| "no answer".into());
                }
                _ => return Err("no response".into()),
            }
        }
    }

    fn mods(&mut self) -> ModsStatus {
        match self.ask(Command::Mods(Empty {})) {
            Ok(Answer::Mods(status)) => status,
            other => panic!("{other:?}"),
        }
    }

    /// Installs `path`, waits for the job to finish and returns its final state.
    fn install(&mut self, path: &Path, mode: InstallMode, replace: bool) -> InstallState {
        let args = InstallArgs {
            path: path.to_string_lossy().into(),
            mode: mode.into(),
            replace,
        };
        let Ok(Answer::StartInstall(job)) = self.ask(Command::StartInstall(args)) else {
            panic!("the install starts");
        };
        for _ in 0..600 {
            if let Some(progress) = self
                .mods()
                .jobs
                .into_iter()
                .find(|other| other.id == job.id)
                && progress.finished.is_some()
            {
                return progress.state();
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("the install ends");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn zip(path: &Path, files: &[&str]) {
    let mut archive = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for file in files {
        archive
            .start_file(*file, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(file.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
}

fn put(path: &Path, data: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

/// Creates the files and folders that make `root` count as an OMSI 2 installation, all empty.
fn omsi(root: &Path) {
    for rel in legacy_config::ORIGINAL_ESSENTIALS {
        if Path::new(rel).extension().is_some() {
            put(&root.join(rel), "");
        } else {
            std::fs::create_dir_all(root.join(rel)).unwrap();
        }
    }
}

fn names(status: &ModsStatus) -> Vec<&str> {
    status
        .installed
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

#[test]
fn the_launcher_installs_orders_and_removes_mods() {
    let dir = std::env::temp_dir().join(format!("neoomsi-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    omsi(&dir.join("omsi"));
    let mut engine = Engine::start(&dir);

    let hello = Handshake {
        protocol_version: "1".into(),
        launcher_version: "contract".into(),
        client_platform: std::env::consts::OS.into(),
    };
    let Ok(Answer::Handshake(handshake)) = engine.ask(Command::Handshake(hello)) else {
        panic!("handshake");
    };
    assert_eq!(handshake.status.unwrap().code(), StatusCode::Ok);
    for command in [
        "start_install",
        "uninstall_mod",
        "allow_mod_plugins",
        "save_mod_order",
        "reset_mod_order",
        "forget_mod",
        "mod_conflicts",
        "map_picture",
        "preflight",
        "diagnostics",
    ] {
        assert!(
            handshake.commands.iter().any(|known| known == command),
            "{command}"
        );
    }

    // An archive whose only top folder is a content folder keeps it; older versions stripped it.
    let bus = dir.join("src/Bus.zip");
    zip(&bus, &["Vehicles/Bus/bus.bus"]);
    assert_eq!(
        engine.install(&bus, InstallMode::Extract, false),
        InstallState::Done
    );
    assert!(dir.join("content/mods/Bus/Vehicles/Bus/bus.bus").is_file());
    let signs = dir.join("src/Signs.zip");
    zip(&signs, &["Pack/Sceneryobjects/Signs/a.sco", "readme.txt"]);
    assert_eq!(
        engine.install(&signs, InstallMode::InPlace, false),
        InstallState::Done
    );

    // A new mod goes to the top. The player's order is kept, and a save from a stale read is refused.
    let status = engine.mods();
    assert_eq!(names(&status), ["Signs.zip", "Bus"]);
    let save = |order: &[&str], revision| {
        Command::SaveModOrder(SaveModOrderArgs {
            order: order.iter().map(|name| name.to_string()).collect(),
            revision,
            new_mods: NewMods::Bottom.into(),
        })
    };
    assert!(
        engine
            .ask(save(&["Bus", "Signs.zip"], status.order_revision + 1))
            .is_err()
    );
    let Ok(Answer::SaveModOrder(status)) =
        engine.ask(save(&["Bus", "Signs.zip"], status.order_revision))
    else {
        panic!("saved");
    };
    assert_eq!(
        (names(&status), status.new_mods()),
        (vec!["Bus", "Signs.zip"], NewMods::Bottom)
    );

    // Installing a mod again needs Replace, and the new version keeps its place.
    assert_eq!(
        engine.install(&bus, InstallMode::Extract, false),
        InstallState::Failed
    );
    assert_eq!(
        engine.install(&bus, InstallMode::Extract, true),
        InstallState::Done
    );
    assert_eq!(names(&engine.mods()), ["Bus", "Signs.zip"]);
    let Ok(Answer::ModConflicts(conflicts)) = engine.ask(Command::ModConflicts(Empty {})) else {
        panic!("conflicts");
    };
    assert!(
        conflicts
            .mods
            .iter()
            .any(|conflict| conflict.name == "Signs.zip" && conflict.files == 1),
        "{conflicts:?}"
    );

    // The shared library ranks below the mods, is never in the order and can't be uninstalled.
    let shared = PathBuf::from(&status.shared_dir);
    assert_eq!(shared, dir.join("content/mods/shared"));
    assert!(shared.is_dir());
    std::fs::create_dir_all(shared.join("Vehicles/Bus")).unwrap();
    std::fs::write(shared.join("Vehicles/Bus/bus.bus"), "shared").unwrap();
    let Ok(Answer::ModConflicts(conflicts)) = engine.ask(Command::ModConflicts(Empty {})) else {
        panic!("conflicts");
    };
    assert!(
        conflicts
            .mods
            .iter()
            .any(|conflict| conflict.name == "shared" && conflict.hidden_by == ["Bus"]),
        "{conflicts:?}"
    );
    assert_eq!(names(&engine.mods()), ["Bus", "Signs.zip"]);
    let remove = Command::UninstallMod(UninstallModArgs {
        name: "shared".into(),
    });
    assert!(engine.ask(remove).is_err());

    // Plugins load only once allowed, and the engine keeps the consent beside the load order.
    put(
        &dir.join("src/Ibis/Plugins/ibis.opl"),
        "[dll]\r\nibis.dll\r\n",
    );
    let ibis = dir.join("src/Ibis");
    assert_eq!(
        engine.install(&ibis, InstallMode::Extract, false),
        InstallState::Done
    );
    let plugins = |status: &ModsStatus| {
        let entry = status
            .installed
            .iter()
            .find(|entry| entry.name == "Ibis")
            .unwrap();
        (entry.has_plugins, entry.plugins_allowed)
    };
    assert_eq!(plugins(&engine.mods()), (true, false));
    let allow = AllowModPluginsArgs {
        name: "Ibis".into(),
        allowed: true,
    };
    assert!(engine.ask(Command::AllowModPlugins(allow)).is_ok());
    assert_eq!(plugins(&engine.mods()), (true, true));
    assert!(dir.join("content/mods/.plugins-allowed.json").is_file());
    let escape = AllowModPluginsArgs {
        name: "../Ibis".into(),
        allowed: true,
    };
    assert!(engine.ask(Command::AllowModPlugins(escape)).is_err());
    // Uninstalling revokes the consent, so moving the same copy back by hand asks again.
    let remove = Command::UninstallMod(UninstallModArgs {
        name: "Ibis".into(),
    });
    assert!(engine.ask(remove).is_ok());
    let uninstalled = dir.join("content/mods/uninstalled");
    let kept = std::fs::read_dir(&uninstalled)
        .unwrap()
        .flatten()
        .find(|dir_entry| dir_entry.file_name().to_string_lossy().ends_with("-Ibis"))
        .unwrap();
    std::fs::rename(kept.path(), dir.join("content/mods/Ibis")).unwrap();
    assert_eq!(plugins(&engine.mods()), (true, false));

    // A map inside a mod: its picture, and the content it names that isn't installed.
    let town = dir.join("src/Town/maps/Town");
    put(&town.join("global.cfg"), "[name]\r\nTown\r\n");
    put(
        &town.join("tile_0_0.map"),
        "[object]\r\n0\r\nSceneryobjects\\Absent\\a.sco\r\n",
    );
    put(&town.join("picture.jpg"), "picture");
    let source = dir.join("src/Town");
    assert_eq!(
        engine.install(&source, InstallMode::Extract, false),
        InstallState::Done
    );
    let map = || "maps/Town/global.cfg".to_string();
    let Ok(Answer::MapPicture(picture)) =
        engine.ask(Command::MapPicture(MapPictureArgs { map: map() }))
    else {
        panic!("picture");
    };
    assert_eq!(picture, b"picture");
    let Ok(Answer::Preflight(preflight)) =
        engine.ask(Command::Preflight(PreflightArgs { map: map() }))
    else {
        panic!("preflight");
    };
    let found: Vec<_> = preflight
        .findings
        .iter()
        .map(|finding| (finding.class(), finding.pack.as_str()))
        .collect();
    assert_eq!(
        found,
        [(DiagnosticClass::MissingDependency, "Sceneryobjects/Absent")]
    );

    // Uninstalling takes a mod out of the order and keeps it in mods/uninstalled, next to the
    // version that Replace set aside.
    let remove = Command::UninstallMod(UninstallModArgs { name: "Bus".into() });
    assert!(engine.ask(remove).is_ok());
    assert_eq!(names(&engine.mods()), ["Signs.zip", "Ibis", "Town"]);
    let kept: Vec<PathBuf> = std::fs::read_dir(dir.join("content/mods/uninstalled"))
        .unwrap()
        .flatten()
        .map(|dir_entry| dir_entry.path())
        .collect();
    assert_eq!(kept.len(), 2);

    // A mod deleted by hand keeps its place until it is forgotten; Reset goes back to A to Z.
    std::fs::remove_dir_all(dir.join("content/mods/Ibis")).unwrap();
    let status = engine.mods();
    assert!(
        status
            .installed
            .iter()
            .any(|entry| entry.name == "Ibis" && !entry.available),
        "{status:?}"
    );
    let forget = ForgetModArgs {
        name: "Ibis".into(),
    };
    let Ok(Answer::ForgetMod(status)) = engine.ask(Command::ForgetMod(forget)) else {
        panic!("forgotten");
    };
    assert_eq!(names(&status), ["Signs.zip", "Town"]);
    let Ok(Answer::SaveModOrder(status)) =
        engine.ask(save(&["Town", "Signs.zip"], status.order_revision))
    else {
        panic!("saved");
    };
    let reset = ResetModOrderArgs {
        revision: status.order_revision,
    };
    let Ok(Answer::ResetModOrder(status)) = engine.ask(Command::ResetModOrder(reset)) else {
        panic!("reset");
    };
    assert_eq!(names(&status), ["Signs.zip", "Town"]);

    // What a game reported missing. A dismissed finding isn't listed again.
    let reported = dir.join("home/.neoomsi/diagnostics/game-1.json");
    std::fs::create_dir_all(reported.parent().unwrap()).unwrap();
    std::fs::write(
        &reported,
        r#"[{"class":"missing_dependency","requester":"Town","pack":"Splines/Hamburg","provider":null,"severity":"required","count":2,"samples":["Splines/Hamburg/a.sli"]}]"#,
    )
    .unwrap();
    let diagnostics = |engine: &mut Engine| {
        let args = DiagnosticsArgs {
            instance: "game-1".into(),
        };
        match engine.ask(Command::Diagnostics(args)) {
            Ok(Answer::Diagnostics(list)) => list.findings,
            other => panic!("{other:?}"),
        }
    };
    let found = diagnostics(&mut engine);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].class(), found[0].pack.as_str()),
        (DiagnosticClass::MissingDependency, "Splines/Hamburg")
    );
    let dismiss = DismissDiagnosticArgs {
        key: found[0].key.clone(),
    };
    assert!(engine.ask(Command::DismissDiagnostic(dismiss)).is_ok());
    assert!(diagnostics(&mut engine).is_empty());
    let traversal = DiagnosticsArgs {
        instance: "../game-1".into(),
    };
    assert!(engine.ask(Command::Diagnostics(traversal)).is_err());

    drop(engine);
    std::fs::remove_dir_all(&dir).ok();
}
