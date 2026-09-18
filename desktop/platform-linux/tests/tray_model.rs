//! T1–T16 — the tray model, with no bus and no desktop.
//!
//! Everything here is a plain function over plain data, which is the point of
//! having split the model out of the D-Bus interfaces: the questions that
//! matter most about a tray — *can a message name an action?*, *can a file
//! name reach the session bus?* — are answered by reading a value, not by
//! standing in front of Plasma with a stopwatch.

#![cfg(feature = "tray")]

use std::collections::HashSet;
use std::path::PathBuf;

use anyflow_linux::tray::model::{
    self, TrayAction, DESKTOP_APP_ID, DESKTOP_APP_OBJECT_PATH, ICON_NAME, ITEM_CATEGORY, ITEM_ID,
    ITEM_STATUS, ITEM_TITLE, MENU, MENU_ROOT_ID, TOOLTIP_BODY, TOOLTIP_TITLE,
};

fn source(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Comment text stripped, so that prose *explaining* a rule is never mistaken
/// for a breach of it. The same trick the Android logging audit uses.
fn code_only(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// T1 — Activate maps only to QuickPanel
// ---------------------------------------------------------------------------

#[test]
fn t1_a_primary_click_is_the_quick_panel_and_only_that() {
    assert_eq!(model::primary_activation(), TrayAction::QuickPanel);
    // Not "usually" the Quick Panel. The function takes no argument, so there
    // is no state it could consult and no second answer it could give.
    for _ in 0..8 {
        assert_eq!(model::primary_activation(), TrayAction::QuickPanel);
    }
}

// ---------------------------------------------------------------------------
// T2, T3, T4 — the three menu rows
// ---------------------------------------------------------------------------

#[test]
fn t2_t3_t4_each_menu_row_maps_to_the_surface_it_names() {
    let by_label = |label: &str| {
        MENU.iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("no menu row labelled {label:?}"))
    };
    assert_eq!(by_label("Quick Panel").action, TrayAction::QuickPanel);
    assert_eq!(by_label("Files").action, TrayAction::Files);
    assert_eq!(by_label("Settings").action, TrayAction::Settings);

    // And the three GApplication action names they resolve to. `Files` maps to
    // `transfers` — the row is named for what a person is looking for, the
    // action for what the GUI has always called the page.
    assert_eq!(TrayAction::QuickPanel.gapplication_action(), "quick-panel");
    assert_eq!(TrayAction::Files.gapplication_action(), "transfers");
    assert_eq!(TrayAction::Settings.gapplication_action(), "settings");
}

#[test]
fn the_menu_is_exactly_the_three_rows_the_sprint_specified() {
    let labels: Vec<&str> = MENU.iter().map(|e| e.label).collect();
    assert_eq!(labels, vec!["Quick Panel", "Files", "Settings"]);

    // Nothing that carries authority, and nothing that stops the daemon. The
    // check is by word rather than by count so that adding "Pair" alongside
    // the three is caught too.
    let forbidden = [
        "Pair",
        "Grant",
        "Revoke",
        "Quit",
        "Exit",
        "Send",
        "Trust",
        "Forget",
        "Disconnect",
    ];
    for label in &labels {
        for word in forbidden {
            assert!(
                !label.contains(word),
                "the tray menu offers {label:?}, which carries authority or stops the daemon"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T5 — an unknown item id does nothing
// ---------------------------------------------------------------------------

#[test]
fn t5_an_unknown_menu_id_resolves_to_no_action_at_all() {
    let known: HashSet<i32> = MENU.iter().map(|e| e.id).collect();
    for id in [-2_147_483_648, -99, -1, 0, 4, 5, 42, 9999, 2_147_483_647] {
        if known.contains(&id) {
            continue;
        }
        assert_eq!(
            model::action_for_menu_id(id),
            None,
            "id {id} resolved to an action"
        );
        assert_eq!(model::action_for_menu_event(id, "clicked"), None);
    }
    // The root is not a row, so choosing "the root" chooses nothing.
    assert_eq!(model::action_for_menu_id(MENU_ROOT_ID), None);
}

// ---------------------------------------------------------------------------
// T6 — the wrong event type does nothing
// ---------------------------------------------------------------------------

#[test]
fn t6_only_a_click_is_a_decision() {
    for entry in MENU {
        assert_eq!(
            model::action_for_menu_event(entry.id, "clicked"),
            Some(entry.action)
        );
        // Everything Plasma's importer actually sends, plus the specification's
        // vendor-extension shape and some noise.
        for event in [
            "hovered",
            "opened",
            "closed",
            "x-kde-anything",
            "Clicked",
            "CLICKED",
            "clicked ",
            "",
            "click",
        ] {
            assert_eq!(
                model::action_for_menu_event(entry.id, event),
                None,
                "{event:?} on row {} was treated as a decision",
                entry.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T7 — the action type cannot hold an arbitrary action name
// ---------------------------------------------------------------------------

#[test]
fn t7_the_action_type_has_no_room_for_a_name_from_the_bus() {
    // A variant carrying a `String`, a `&str` or a `Box<str>` could not be one
    // byte wide. This is the cheapest possible proof that `TrayAction` is a
    // tag and not a container.
    assert_eq!(std::mem::size_of::<TrayAction>(), 1);

    // And the set of names it can produce is closed and enumerable.
    let produced: HashSet<&str> = TrayAction::ALL
        .iter()
        .map(|a| a.gapplication_action())
        .collect();
    assert_eq!(
        produced,
        HashSet::from(["quick-panel", "transfers", "settings"])
    );

    // The source half: nothing in the tray constructs an action name from a
    // value. `gapplication_action` returns `&'static str` from a `match` over
    // three variants, and no file in the tray formats one.
    for file in [
        "src/tray/model.rs",
        "src/tray/menu.rs",
        "src/tray/item.rs",
        "src/tray/activate.rs",
        "src/tray/watcher.rs",
    ] {
        let code = code_only(&source(file));
        for pattern in [
            "ActivateAction\", &(event",
            "ActivateAction\", &(name",
            "gapplication_action(&",
            "format!(\"app.",
        ] {
            assert!(
                !code.contains(pattern),
                "{file} builds an action name from a value: {pattern}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T8, T9, T10 — the ids and the layout
// ---------------------------------------------------------------------------

#[test]
fn t8_menu_ids_are_unique_and_none_of_them_is_the_root() {
    let ids: Vec<i32> = MENU.iter().map(|e| e.id).collect();
    let unique: HashSet<i32> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "two menu rows share an id");
    assert!(
        !unique.contains(&MENU_ROOT_ID),
        "a row claims the root's id, which is what a host asks for the whole menu with"
    );
}

#[test]
fn t9_menu_ids_are_the_values_they_have_always_been() {
    // A golden. The point of an id is that a shell can hold on to it, so
    // changing one is a protocol change to anything that already has it
    // cached — and it is the kind of change that happens by accident, when a
    // row is reordered, unless a test writes the numbers down.
    let pairs: Vec<(i32, &str)> = MENU.iter().map(|e| (e.id, e.label)).collect();
    assert_eq!(
        pairs,
        vec![(1, "Quick Panel"), (2, "Files"), (3, "Settings")]
    );
    assert_eq!(MENU_ROOT_ID, 0);
}

#[test]
fn t10_the_layout_is_deterministic_and_is_not_derived_from_position() {
    // Same order, same ids, every time it is read.
    let once: Vec<(i32, &str)> = MENU.iter().map(|e| (e.id, e.label)).collect();
    for _ in 0..16 {
        let again: Vec<(i32, &str)> = MENU.iter().map(|e| (e.id, e.label)).collect();
        assert_eq!(once, again);
    }
    // And the lookup is by id, not by index. A model that did
    // `MENU[id as usize]` would pass every test above and would hand a host
    // the wrong row the first time a row was inserted.
    let code = code_only(&source("src/tray/model.rs"));
    assert!(
        !code.contains("MENU["),
        "the menu is indexed by position somewhere"
    );
    assert!(
        code.contains("find(|e| e.id == id)"),
        "the id lookup is no longer a search by id"
    );
}

// ---------------------------------------------------------------------------
// T11 — the tooltip carries nothing about anybody
// ---------------------------------------------------------------------------

#[test]
fn t11_nothing_on_the_public_tray_object_is_about_a_person_or_a_file() {
    // The full set of strings this process is willing to put on the session
    // bus, gathered in one place so that the assertion is about all of them
    // rather than about the two somebody remembered.
    let published = [
        ITEM_ID,
        ITEM_TITLE,
        ITEM_CATEGORY,
        ITEM_STATUS,
        ICON_NAME,
        TOOLTIP_TITLE,
        TOOLTIP_BODY,
        MENU[0].label,
        MENU[1].label,
        MENU[2].label,
    ];
    let allowed: HashSet<&str> = HashSet::from([
        "io.github.yurisismotto.anyflow",
        "AnyFlow",
        "ApplicationStatus",
        "Active",
        "One flow. Any device.",
        "Quick Panel",
        "Files",
        "Settings",
    ]);
    for value in published {
        assert!(
            allowed.contains(value),
            "the tray publishes {value:?}, which is not on the list of strings this \
             product has decided may leave the process without being asked for"
        );
    }

    // And the source of those strings names nothing that could carry content.
    // Every one of them is a literal in `model.rs`; none is a parameter, a
    // format string or a read of anything.
    let code = code_only(&source("src/tray/model.rs"));
    for forbidden in [
        "peer",
        "Peer",
        "fingerprint",
        "Fingerprint",
        "clipboard",
        "Clipboard",
        "filename",
        "device_name",
        "battery",
        "notification",
        "format!",
    ] {
        assert!(
            !code.contains(forbidden),
            "the tray model mentions {forbidden:?}"
        );
    }
    // "transfer" is the one word on that list that legitimately appears, and
    // only as the name of the GAction the GUI has exported since the Quick
    // Panel sprint. It must never appear as anything else — a transfer's
    // filename, size or peer reaching a menu label would match here.
    let transfer_mentions: Vec<&str> = code
        .lines()
        .filter(|l| l.contains("transfer"))
        .map(|l| l.trim())
        .collect();
    assert_eq!(
        transfer_mentions,
        vec!["TrayAction::Files => \"transfers\","],
        "the tray model mentions a transfer somewhere other than the action name"
    );
}

// ---------------------------------------------------------------------------
// T12 — identity
// ---------------------------------------------------------------------------

#[test]
fn t12_the_icon_and_the_item_id_are_the_application_id() {
    assert_eq!(ICON_NAME, "io.github.yurisismotto.anyflow");
    assert_eq!(ICON_NAME, DESKTOP_APP_ID);
    assert_eq!(ITEM_ID, DESKTOP_APP_ID);
    assert_eq!(ITEM_TITLE, "AnyFlow");

    // A theme name, not a path and not a file. The three things it must not
    // look like are an absolute path, a URL and a filename with an extension.
    assert!(!ICON_NAME.starts_with('/'));
    assert!(!ICON_NAME.contains("://"));
    assert!(!ICON_NAME.ends_with(".svg") && !ICON_NAME.ends_with(".png"));
    assert!(!ICON_NAME.contains("/tmp") && !ICON_NAME.contains("home"));

    // The object path is the id, mechanically.
    assert_eq!(
        DESKTOP_APP_OBJECT_PATH,
        format!("/{}", DESKTOP_APP_ID.replace('.', "/"))
    );
}

// ---------------------------------------------------------------------------
// T13 — the status is truthful
// ---------------------------------------------------------------------------

#[test]
fn t13_the_status_is_active_and_never_asks_for_attention() {
    assert_eq!(ITEM_STATUS, "Active");
    assert_ne!(ITEM_STATUS, "NeedsAttention");
    assert_ne!(ITEM_STATUS, "Passive");

    // Nothing in the tray can produce `NeedsAttention` at all: the string does
    // not appear outside a comment explaining why it does not.
    for file in ["src/tray/model.rs", "src/tray/item.rs", "src/tray/menu.rs"] {
        let code = code_only(&source(file));
        assert!(
            !code.contains("NeedsAttention"),
            "{file} can produce NeedsAttention"
        );
    }

    // The attention icon and the animation are empty for the same reason, and
    // that is visible in the item rather than assumed.
    let item = code_only(&source("src/tray/item.rs"));
    assert!(item.contains("fn attention_icon_name"));
    assert!(item.contains("fn attention_movie_name"));
}

// ---------------------------------------------------------------------------
// T14 — scroll, and the other harmless interactions
// ---------------------------------------------------------------------------

#[test]
fn t14_scroll_and_the_other_unspecified_gestures_have_empty_bodies() {
    // A model-level assertion cannot call the D-Bus method, so this reads the
    // implementation: the three handlers whose correct behaviour is "nothing"
    // must have nothing in them. A regression here would be someone wiring a
    // wheel event to a device or a transfer, which is precisely the mistake
    // this is here to make impossible to land quietly.
    let item = source("src/tray/item.rs");
    for handler in [
        "fn scroll(&self, _delta: i32, _orientation: String) {}",
        "fn secondary_activate(&self, _x: i32, _y: i32) {}",
        "fn context_menu(&self, _x: i32, _y: i32) {}",
    ] {
        assert!(
            item.contains(handler),
            "the handler `{handler}` is no longer empty"
        );
    }
}

// ---------------------------------------------------------------------------
// T15 — an activation failure changes nothing
// ---------------------------------------------------------------------------

#[test]
fn t15_nothing_about_the_menu_depends_on_an_activation_having_worked() {
    // The menu is a constant, so there is no state an activation could
    // mutate. That is the strongest form this assertion can take, and it is
    // structural: `MENU` is a `const`, the ids are literals, and the two
    // interface types hold an activator and nothing else that a click writes.
    let menu = code_only(&source("src/tray/menu.rs"));
    assert!(
        menu.contains("pub struct TrayMenu {")
            && menu.contains("activator: Arc<dyn ApplicationActivator>,"),
        "the menu grew a field"
    );
    // One field. A second one would be state, and state is what an activation
    // failure could corrupt.
    let body_start = menu
        .find("pub struct TrayMenu {")
        .expect("the menu struct is declared");
    let body_end = menu[body_start..]
        .find('}')
        .expect("the menu struct is closed")
        + body_start;
    let fields = menu[body_start..body_end]
        .lines()
        .filter(|l| l.contains(':'))
        .count();
    assert_eq!(fields, 1, "TrayMenu holds mutable state");

    // The D-Bus half of this — a failing activator, a real bus, and a menu
    // that still answers — is `tray_dbus.rs::d8_…` and `d12_…`.
}

// ---------------------------------------------------------------------------
// T16 — nothing routes by name, fingerprint or position
// ---------------------------------------------------------------------------

#[test]
fn t16_no_tray_decision_is_made_by_a_name_or_a_position() {
    for file in [
        "src/tray/model.rs",
        "src/tray/menu.rs",
        "src/tray/item.rs",
        "src/tray/activate.rs",
        "src/tray/watcher.rs",
        "src/tray/mod.rs",
    ] {
        let code = code_only(&source(file));
        for pattern in [
            "label ==",
            "label.eq",
            ".position(",
            "MENU[",
            "peer",
            "fingerprint",
            "Fingerprint",
        ] {
            assert!(!code.contains(pattern), "{file} routes on {pattern:?}");
        }
    }
}
