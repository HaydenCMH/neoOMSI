//! The launcher's contract with the engine, end to end: `neoomsi --control-protocol`, started
//! the way the launcher starts it, installs, orders and removes mods in a content folder of its
//! own (and a home of its own: nothing of the machine's is read or written).

use launcher_protocol::api::{
    DiagnosticsArgs, DismissDiagnosticArgs, Empty, Frame, Handshake, InstallArgs, InstallMode,
    ModsStatus, Request, SaveModOrderArgs,
    StatusCode, UninstallModArgs, frame::Body, request::Command, response::Answer,
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
            .env_remove("OMSI_ROOT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the engine starts");
        let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        Engine { child, stdin, stdout, next: 0 }
    }

    /// The answer to `command` (events on the way are skipped).
    fn ask(&mut self, command: Command) -> Result<Answer, String> {
        self.next += 1;
        let id = format!("t{}", self.next);
        let request = Frame {
            request_id: id.clone(),
            body: Some(Body::Request(Request { command: Some(command) })),
            ..Default::default()
        };
        write_frame(&mut self.stdin, &request).unwrap();
        loop {
            let f: Frame = read_frame(&mut self.stdout).unwrap().expect("the engine answers");
            if f.request_id != id {
                continue;
            }
            if !f.error.is_empty() {
                return Err(f.error);
            }
            match f.body {
                Some(Body::Response(r)) => return r.answer.ok_or_else(|| "no answer".into()),
                _ => return Err("no response".into()),
            }
        }
    }

    fn mods(&mut self) -> ModsStatus {
        match self.ask(Command::Mods(Empty {})) {
            Ok(Answer::Mods(m)) => m,
            other => panic!("{other:?}"),
        }
    }

    /// Install `path` and wait for the job to end; its state.
    fn install(&mut self, path: &Path, mode: InstallMode) -> i32 {
        let args = InstallArgs {
            path: path.to_string_lossy().into(),
            mode: mode.into(),
            replace: false,
        };
        let Ok(Answer::StartInstall(job)) = self.ask(Command::StartInstall(args)) else {
            panic!("the install starts");
        };
        for _ in 0..600 {
            if let Some(j) = self.mods().jobs.into_iter().find(|j| j.id == job.id) {
                if j.finished.is_some() {
                    return j.state;
                }
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
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for f in files {
        z.start_file(*f, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(f.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

fn names(m: &ModsStatus) -> Vec<&str> {
    m.installed.iter().map(|i| i.name.as_str()).collect()
}

#[test]
fn the_launcher_installs_orders_and_removes_mods() {
    let dir = std::env::temp_dir().join(format!("neoomsi-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut engine = Engine::start(&dir);

    let hello = Handshake {
        protocol_version: "1".into(),
        launcher_version: "contract".into(),
        client_platform: std::env::consts::OS.into(),
    };
    let Ok(Answer::Handshake(h)) = engine.ask(Command::Handshake(hello)) else {
        panic!("handshake");
    };
    assert_eq!(h.status.unwrap().code(), StatusCode::Ok);
    for c in ["start_install", "uninstall_mod", "save_mod_order", "mod_conflicts", "diagnostics"] {
        assert!(h.commands.iter().any(|x| x == c), "{c}");
    }

    // an archive whose only top folder is a content folder keeps it (it was stripped once)
    let bus = dir.join("src/Bus.zip");
    zip(&bus, &["Vehicles/Bus/bus.bus"]);
    assert_eq!(engine.install(&bus, InstallMode::Extract), 7, "done");
    assert!(dir.join("content/mods/Bus/Vehicles/Bus/bus.bus").is_file());
    let signs = dir.join("src/Signs.zip");
    zip(&signs, &["Pack/Sceneryobjects/Signs/a.sco", "readme.txt"]);
    assert_eq!(engine.install(&signs, InstallMode::InPlace), 7, "done");

    // a new mod goes to the top; the player's order is kept, a stale one refused
    let m = engine.mods();
    assert_eq!(names(&m), ["Signs.zip", "Bus"]);
    let save = |revision| {
        Command::SaveModOrder(SaveModOrderArgs {
            order: vec!["Bus".into(), "Signs.zip".into()],
            revision,
            new_mods: "bottom".into(),
        })
    };
    assert!(engine.ask(save(m.order_revision + 1)).is_err());
    let Ok(Answer::SaveModOrder(m)) = engine.ask(save(m.order_revision)) else {
        panic!("saved");
    };
    assert_eq!((names(&m), m.new_mods.as_str()), (vec!["Bus", "Signs.zip"], "bottom"));
    let Ok(Answer::ModConflicts(c)) = engine.ask(Command::ModConflicts(Empty {})) else {
        panic!("conflicts");
    };
    assert!(c.mods.iter().any(|x| x.name == "Signs.zip" && x.files == 1), "{c:?}");

    // uninstalled: out of the order, kept in mods/uninstalled
    let remove = Command::UninstallMod(UninstallModArgs { name: "Bus".into() });
    assert!(engine.ask(remove).is_ok());
    assert_eq!(names(&engine.mods()), ["Signs.zip"]);
    let kept: Vec<PathBuf> = std::fs::read_dir(dir.join("content/mods/uninstalled"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(kept.len(), 1);

    // what a game reported missing; a dismissed finding is not listed again
    let reported = dir.join("home/.neoomsi/diagnostics/game-1.json");
    std::fs::create_dir_all(reported.parent().unwrap()).unwrap();
    std::fs::write(
        &reported,
        r#"[{"class":"missing_dependency","requester":"Town","pack":"Splines/Hamburg","provider":null,"severity":"required","count":2,"samples":["Splines/Hamburg/a.sli"]}]"#,
    )
    .unwrap();
    let diagnostics = |engine: &mut Engine| {
        let args = DiagnosticsArgs { instance: "game-1".into() };
        match engine.ask(Command::Diagnostics(args)) {
            Ok(Answer::Diagnostics(d)) => d.findings,
            other => panic!("{other:?}"),
        }
    };
    let found = diagnostics(&mut engine);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].class.as_str(), found[0].pack.as_str()), ("missing_dependency", "Splines/Hamburg"));
    let dismiss = DismissDiagnosticArgs { key: found[0].key.clone() };
    assert!(engine.ask(Command::DismissDiagnostic(dismiss)).is_ok());
    assert!(diagnostics(&mut engine).is_empty());
    let traversal = DiagnosticsArgs { instance: "../game-1".into() };
    assert!(engine.ask(Command::Diagnostics(traversal)).is_err());

    drop(engine);
    std::fs::remove_dir_all(&dir).ok();
}
