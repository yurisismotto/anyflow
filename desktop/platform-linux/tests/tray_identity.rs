//! The tray's constants, against the files the desktop session actually
//! reads.
//!
//! # Why this test is in *this* crate
//!
//! The tray names four things that live somewhere else: the GtkApplication's
//! id, the desktop entry, the D-Bus service file and the icon in the hicolor
//! theme. `anyflow-linux` cannot depend on `anyflow-gui` — the dependency runs
//! the other way, and it must, because the daemon would otherwise link GTK —
//! so the agreement cannot be checked by the compiler. It is checked here, by
//! reading the other crate's files.
//!
//! A mismatch is invisible in every other way. A tray item that asked for an
//! icon name nothing installs draws a grey square; one that named an action
//! the GUI does not export produces a click that does nothing, an error nobody
//! is watching for, and no other symptom at all.

#![cfg(feature = "tray")]

use std::path::{Path, PathBuf};

use anyflow_linux::tray::model::{TrayAction, DESKTOP_APP_ID, ICON_NAME, ITEM_ID};

fn gui() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../gui")
}

fn read(relative: &str) -> String {
    let path = gui().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The application id as the GUI declares it, read out of its source.
fn app_id_from_gui_source() -> String {
    read("src/lib.rs")
        .lines()
        .find(|l| l.trim_start().starts_with("const APP_ID:"))
        .expect("anyflow-gui declares APP_ID")
        .split('"')
        .nth(1)
        .expect("APP_ID is a string literal")
        .to_string()
}

fn key(text: &str, group: &str, key: &str) -> Option<String> {
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == group;
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix(&format!("{key}=")) {
            return Some(value.to_string());
        }
    }
    None
}

#[test]
fn the_tray_names_the_application_the_gui_actually_is() {
    let app_id = app_id_from_gui_source();
    assert_eq!(
        DESKTOP_APP_ID, app_id,
        "the tray activates a bus name the GUI does not own"
    );
    assert_eq!(ITEM_ID, app_id);
    assert_eq!(
        ICON_NAME, app_id,
        "the tray asks the shell for an icon name nothing installs"
    );
}

#[test]
fn the_tray_icon_name_is_the_one_the_desktop_entry_declares() {
    let app_id = app_id_from_gui_source();
    let entry = read(&format!("data/{app_id}.desktop"));
    assert_eq!(
        key(&entry, "[Desktop Entry]", "Icon").as_deref(),
        Some(ICON_NAME),
        "the tray and the launcher would draw different icons"
    );
    // And the installer writes that name into the theme the shell searches.
    let installer = read("tools/install-desktop-metadata.sh");
    assert!(installer.contains("share/icons/hicolor/scalable/apps"));
    assert!(installer.contains("$APP_ID.svg"));
}

#[test]
fn the_tray_activates_a_bus_name_the_session_bus_can_start() {
    let app_id = app_id_from_gui_source();
    let service = read(&format!("data/{app_id}.service.in"));
    assert_eq!(
        key(&service, "[D-BUS Service]", "Name").as_deref(),
        Some(DESKTOP_APP_ID),
        "the bus cannot start the name the tray calls"
    );
    // Cold activation is the case the tray exists for: a freshly booted
    // session where `anyflowd` runs as a user service and no GUI process
    // exists at all.
    assert!(key(&service, "[D-BUS Service]", "Exec")
        .expect("Exec")
        .ends_with(" --gapplication-service"));
}

#[test]
fn every_tray_action_is_an_action_the_gui_exports() {
    // The GUI's own public constants, read from its source. Restating the
    // three strings here would make this file agree with itself.
    let lib = read("src/lib.rs");
    let constant = |name: &str| {
        lib.lines()
            .find(|l| l.trim_start().starts_with(&format!("pub const {name}:")))
            .unwrap_or_else(|| panic!("anyflow-gui declares {name}"))
            .split('"')
            .nth(1)
            .expect("a string literal")
            .to_string()
    };
    assert_eq!(
        TrayAction::QuickPanel.gapplication_action(),
        constant("ACTION_QUICK_PANEL")
    );
    assert_eq!(
        TrayAction::Settings.gapplication_action(),
        constant("ACTION_SETTINGS")
    );
    assert_eq!(
        TrayAction::Files.gapplication_action(),
        constant("ACTION_TRANSFERS")
    );

    // And the GUI really installs all three on the application, rather than
    // declaring the names and wiring two of them.
    for name in ["ACTION_QUICK_PANEL", "ACTION_SETTINGS", "ACTION_TRANSFERS"] {
        assert!(
            lib.contains(&format!("gio::SimpleAction::new({name}, None)")),
            "{name} is declared but never installed as an application action"
        );
    }

    // Parameterless, all three. A tray that had to pass a parameter would be a
    // tray that had to know a variant type, and the GUI documents keeping them
    // parameterless for exactly this caller.
    assert_eq!(lib.matches("SimpleAction::new(ACTION_").count(), 3);
}

#[test]
fn the_daemon_starts_the_tray_and_does_not_race_it_against_anything() {
    // The supervision decision, asserted where it can be read. `anyflowd`
    // races the network listener, the control server and `ctrl_c` in a
    // `select!`, and the first of those to finish ends the process. The tray
    // must not be in that race: it is convenience, and convenience must not be
    // able to stop a file transfer.
    let main = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../daemon/src/main.rs"),
    )
    .expect("the daemon's main is readable");

    assert!(
        main.contains("anyflow_linux::tray::spawn("),
        "the daemon no longer starts the tray"
    );

    let select = main
        .split("tokio::select!")
        .nth(1)
        .expect("the daemon has a select!");
    let select = &select[..select.find('}').unwrap_or(select.len())];
    assert!(
        !select.contains("tray"),
        "the tray is in the daemon's select!; its failure would end the process"
    );

    // And it is not spawning a process. The whole point of activating over
    // the bus is that the GUI does not inherit `anyflowd`'s sandbox.
    for forbidden in [
        "Command::new",
        "std::process::Command",
        "xdg-open",
        "gtk-launch",
        "sh -c",
    ] {
        for file in [
            "src/tray/mod.rs",
            "src/tray/activate.rs",
            "src/tray/item.rs",
            "src/tray/menu.rs",
            "src/tray/watcher.rs",
        ] {
            let code: String =
                std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
                    .expect("the tray source is readable")
                    .lines()
                    .filter(|l| {
                        let t = l.trim_start();
                        !t.starts_with("//") && !t.starts_with('*')
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            assert!(
                !code.contains(forbidden),
                "{file} launches a child process ({forbidden}); it would inherit the \
                 daemon's systemd sandbox"
            );
        }
    }
}
