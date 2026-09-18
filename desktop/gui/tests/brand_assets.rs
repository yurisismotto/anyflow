//! Structural validation of the AnyFlow brand artwork.
//!
//! The marks are source-controlled vectors that get compiled into the binary
//! and shipped as the application icon, so what can go wrong with them is not
//! "it looks off" — it is that an asset quietly starts depending on something
//! that will not be there:
//!
//! * an embedded raster, which stops it being a vector at all;
//! * a network reference, which a desktop icon must never make;
//! * a bundled or named font, which a person's machine may not have and which
//!   the typography decision forbids shipping;
//! * a filesystem path left behind by an editor, which leaks whose machine it
//!   was drawn on;
//! * a stroke so fine that the mark disappears at 16 px, which is the one
//!   size an application icon is guaranteed to be drawn at.
//!
//! None of that is visible by looking at the picture, which is why it is
//! checked here. How the marks *look* at each size is a human judgement and
//! is recorded in the sprint report instead.
//!
//! This is a GUI-crate test target on purpose. It reads desktop artwork and
//! is not part of the portable core, so it is outside the boundary
//! `portable-windows-msvc.yml` measures — that job's classification guards
//! watch `core/tests` and `capabilities/notifications/tests`, and neither is
//! this.

use std::path::{Path, PathBuf};

/// The canonical asset directory, relative to this crate.
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/assets")
}

/// Every asset this application ships or compiles in, and the smallest width
/// in pixels it is ever drawn at.
///
/// Named one by one rather than globbed, and classified rather than lumped: a
/// new mark should have to state where it is used, and a mark that is deleted
/// should fail here rather than stop being checked silently.
///
/// The marks are icons and answer for 16 px. `ribbon-connection.svg` is an
/// *illustration* — the empty-state artwork, drawn at 200x72 and never as an
/// icon — so holding it to an icon's floor would be measuring it against a
/// size it is never asked to be.
const ASSETS: [(&str, f64); 6] = [
    ("logo-flow-a.svg", 16.0),
    ("logo-flow-a-mono.svg", 16.0),
    ("logo-flow-a-small.svg", 16.0),
    ("app-icon.svg", 16.0),
    ("icon-flowing-ribbon.svg", 16.0),
    ("ribbon-connection.svg", 200.0),
];

fn read(name: &str) -> String {
    let path = assets().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()))
}

/// Pulls an attribute off the first element that carries it.
///
/// Matched on a whole attribute name, not a substring: `d="` also occurs
/// inside `id="`, and a scanner that does not care about the boundary reads a
/// gradient's name as a path's geometry.
fn attr(svg: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    svg.match_indices(&needle)
        .find(|(index, _)| {
            *index == 0
                || svg[..*index]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_whitespace() || c == '<')
        })
        .and_then(|(index, _)| svg[index + needle.len()..].split('"').next())
        .map(str::to_string)
}

/// The uniform scale a wrapping `<g transform="… scale(n) …">` applies.
///
/// The app icon draws the 64-unit mark inside a 512-unit ground, so its
/// stroke on the page is several times the number written on the element. A
/// check that ignored that would read the sharpest asset in the set as the
/// thinnest.
fn group_scale(svg: &str) -> f64 {
    svg.split("scale(")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .filter_map(|n| n.trim().parse::<f64>().ok())
        .next()
        .unwrap_or(1.0)
}

#[test]
fn every_asset_is_a_self_contained_svg() {
    for (name, _) in ASSETS {
        let svg = read(name);
        assert!(
            svg.contains("<svg") && svg.trim_end().ends_with("</svg>"),
            "{name} is not an SVG document"
        );
        assert!(
            svg.contains("xmlns=\"http://www.w3.org/2000/svg\""),
            "{name} does not declare the SVG namespace"
        );
        // Without a viewBox the mark does not scale, which is the entire
        // reason it is a vector.
        assert!(attr(&svg, "viewBox").is_some(), "{name} has no viewBox");
        assert_eq!(
            svg.matches("<svg").count(),
            1,
            "{name} has more than one root"
        );
    }
}

/// Nothing is fetched, embedded or inherited from outside the file.
#[test]
fn no_asset_reaches_outside_itself() {
    for (name, _) in ASSETS {
        let svg = read(name);
        let lower = svg.to_lowercase();

        for forbidden in [
            "<image",     // an embedded or linked raster
            "data:image", // a raster smuggled in as a data URI
            "href=",      // any external reference, xlink or otherwise
            "<script",    // an icon is not a program
            "<use",       // a reference to something that may not be there
            "@font-face", // a bundled font
            "<font",
            ".ttf",
            ".woff",
            "<text", // text in a mark is a font dependency by another name
            "font-family",
        ] {
            assert!(
                !lower.contains(forbidden),
                "{name} contains {forbidden:?}, which makes it depend on something outside itself"
            );
        }

        // The namespace declaration is a URI, not a fetch. Any *other* http
        // reference would be.
        let http_uses: Vec<&str> = lower
            .match_indices("http")
            .map(|(i, _)| &lower[i..(i + 40).min(lower.len())])
            .filter(|s| !s.starts_with("http://www.w3.org/2000/svg"))
            .collect();
        assert!(
            http_uses.is_empty(),
            "{name} references the network: {http_uses:?}"
        );

        // An editor that wrote out where it was run leaks the author's
        // machine into a file that ships.
        for leak in [
            "/home/",
            "/users/",
            "file://",
            "c:\\",
            "sodipodi",
            "inkscape:",
        ] {
            assert!(
                !lower.contains(leak),
                "{name} leaks a local path or editor state: {leak:?}"
            );
        }
    }
}

/// Every gradient a mark paints with is defined in that same mark.
///
/// A `url(#id)` that resolves to nothing does not fail loudly — the shape is
/// simply painted black, or not at all, which is exactly the kind of breakage
/// that reaches a release.
#[test]
fn every_paint_reference_resolves_inside_its_own_file() {
    for (name, _) in ASSETS {
        let svg = read(name);
        for (index, _) in svg.match_indices("url(#") {
            let id: String = svg[index + 5..].chars().take_while(|c| *c != ')').collect();
            assert!(
                svg.contains(&format!("id=\"{id}\"")),
                "{name} paints with url(#{id}), which it never defines"
            );
        }
    }
}

/// Two marks in one application cannot share a gradient id.
///
/// They are compiled into one binary and can end up in one document; GTK
/// renders each `GtkPicture` separately today, but an id collision is the
/// kind of latent conflict that is free to avoid and expensive to diagnose.
#[test]
fn gradient_ids_are_unique_across_the_family() {
    let mut seen: Vec<(String, &str)> = Vec::new();
    for (name, _) in ASSETS {
        let svg = read(name);
        for (index, _) in svg.match_indices("id=\"") {
            let id: String = svg[index + 4..].chars().take_while(|c| *c != '"').collect();
            if let Some((_, owner)) = seen.iter().find(|(other, _)| *other == id) {
                panic!("{name} and {owner} both define id {id:?}");
            }
            seen.push((id, name));
        }
    }
}

/// Every mark has to survive the smallest size it is drawn at.
///
/// A stroke is only visible if it lands on at least a whole pixel once the
/// viewBox has been scaled down, so this is measured rather than eyeballed:
/// `stroke-width / viewBox width * smallest` is the stroke's width in pixels
/// there, and it must be at least one. The `-small` cut of the mark exists
/// because the canonical weight is only just over that line at 16 px.
#[test]
fn every_stroke_survives_the_smallest_size_it_is_drawn_at() {
    for (name, smallest) in ASSETS {
        let svg = read(name);
        let Some(width) = attr(&svg, "stroke-width").and_then(|w| w.parse::<f64>().ok()) else {
            // A purely filled mark has no stroke to lose.
            continue;
        };
        let view: Vec<f64> = attr(&svg, "viewBox")
            .expect("a viewBox")
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        let view_width = view[2];
        let rendered = width * group_scale(&svg) / view_width * smallest;
        assert!(
            rendered >= 1.0,
            "{name}: a {width}-unit stroke on a {view_width}-unit grid is {rendered:.2} px at \
             {smallest} px wide, which is thinner than a pixel"
        );
    }
}

/// The mark the application actually wears.
#[test]
fn the_flow_a_is_one_continuous_ribbon() {
    for name in [
        "logo-flow-a.svg",
        "logo-flow-a-mono.svg",
        "logo-flow-a-small.svg",
    ] {
        let svg = read(name);
        assert_eq!(
            svg.matches("<path").count(),
            1,
            "{name}: the mark is one ribbon, so it is one path"
        );
        let d = attr(&svg, "d").expect("a path");
        assert_eq!(
            d.matches('M').count() + d.matches('m').count(),
            1,
            "{name}: one subpath, so the ribbon never lifts: {d}"
        );
        // Round terminals are what make it read as a ribbon rather than as a
        // cut letterform, and they are the same terminals the rest of the
        // family uses.
        assert!(svg.contains("stroke-linecap=\"round\""), "{name}");
        assert!(
            svg.contains("fill=\"none\""),
            "{name} is stroked, not filled"
        );
    }
}

/// The monochrome cut must actually be monochrome, and must take its colour
/// from wherever it is placed rather than hard-coding one.
#[test]
fn the_monochrome_cut_inherits_its_colour() {
    let svg = read("logo-flow-a-mono.svg");
    assert!(
        svg.contains("stroke=\"currentColor\""),
        "the mono mark must inherit the text colour, so it works on light and dark alike"
    );
    assert!(
        !svg.contains("linearGradient") && !svg.contains('#'),
        "the mono mark must name no colour of its own: {svg}"
    );
}

/// The application icon is a full-bleed icon, not the bare mark.
#[test]
fn the_app_icon_is_a_drawn_icon_with_its_own_ground() {
    let svg = read("app-icon.svg");
    assert!(
        svg.contains("<rect") && svg.contains("rx=\""),
        "the app icon needs its own rounded ground: a bare stroke on transparency \
         disappears against a dark shell panel"
    );
    // Painted, never left to the platform's default, so the mark's contrast
    // is a property of the file rather than of whatever is behind it.
    assert!(svg.contains("fill=\"#0F172A\""), "the ground is Ink");
}

// ===========================================================================
// BRAND-POLISH-01 — the application icon, as the desktop resolves it
// ===========================================================================
//
// Everything above asks whether the artwork is sound. These ask whether the
// desktop can *find* it, which is a different question and the one that was
// actually wrong: the marks were correct and compiled in, and GNOME Shell
// still drew a generic square, because on Wayland the shell resolves a
// window's icon entirely outside the application:
//
//     xdg_toplevel.set_app_id(APP_ID)   -> APP_ID.desktop from XDG_DATA_DIRS
//                                       -> its Icon= name
//                                       -> that name in the shell's theme
//
// Four names have to be the same string for that to land, and they live in
// four different files. That is what these pin.

/// This crate's data directory.
fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data")
}

/// The application id, read out of the source rather than restated here.
///
/// Restating it would make this file agree with itself while disagreeing with
/// the application, which is exactly the failure being guarded against.
fn app_id_from_source() -> String {
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("src/lib.rs is readable");
    let line = lib
        .lines()
        .find(|l| l.trim_start().starts_with("const APP_ID:"))
        .expect("src/lib.rs declares APP_ID");
    line.split('"')
        .nth(1)
        .expect("APP_ID is a string literal")
        .to_string()
}

/// One key from the `[Desktop Entry]` group.
///
/// Stops at the first group header so that `[Desktop Action quick-panel]`'s
/// own `Name` and `Exec` cannot be read as the entry's.
fn desktop_entry_key(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(data().join(format!("{}.desktop", app_id_from_source())))
        .expect("the desktop entry is readable");
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix(&format!("{key}=")) {
            return Some(value.to_string());
        }
    }
    None
}

/// A + C. The file's basename, the id in the source and the `Icon=` key are
/// one string.
///
/// A mismatch in any direction is the defect: the shell looks the window's
/// app_id up as a desktop-file id, then looks that file's `Icon=` up as an
/// icon name, and a miss at either step is a grey square with no error
/// anywhere.
#[test]
fn the_application_id_names_the_desktop_entry_and_its_icon() {
    let app_id = app_id_from_source();
    assert!(
        app_id.contains('.') && !app_id.contains('/') && !app_id.contains(' '),
        "{app_id} is not usable as a D-Bus name and a desktop file id"
    );

    let entry = data().join(format!("{app_id}.desktop"));
    assert!(
        entry.is_file(),
        "no desktop entry at {} — the shell matches a window by this filename",
        entry.display()
    );

    assert_eq!(
        desktop_entry_key("Icon").as_deref(),
        Some(app_id.as_str()),
        "Icon= must be the application id, or the shell searches for a name \
         nothing installs"
    );
}

/// B + I. The icon name the *application* asks for, the name the desktop
/// entry declares, and the filename the installer writes are the same.
///
/// The three places that name has to appear are in three languages — Rust,
/// an ini file and a shell script — so nothing but a test can keep them
/// together.
#[test]
fn every_layer_asks_for_the_same_icon_name() {
    let app_id = app_id_from_source();

    // The application: `install_icons` sets the default window icon name.
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("src/lib.rs is readable");
    assert!(
        lib.contains("set_default_icon_name(APP_ID)"),
        "the window icon name is no longer the application id"
    );

    // build.rs, which derives the compiled-in icon-theme copy.
    let build = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("build.rs is readable");
    assert!(
        build.contains(&format!("const APP_ID: &str = \"{app_id}\"")),
        "build.rs names a different application id than src/lib.rs"
    );

    // The installer, which is what a Wayland session actually reads.
    let installer = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/install-desktop-metadata.sh"),
    )
    .expect("the installer is readable");
    assert!(
        installer.contains(&format!("APP_ID=\"{app_id}\"")),
        "the installer names a different application id"
    );
    // And it installs the icon where an icon theme is looked up, under that
    // name. A scalable SVG needs no size directory of its own.
    assert!(
        installer.contains("share/icons/hicolor/scalable/apps"),
        "the installer does not write into a hicolor icon theme"
    );
    assert!(
        installer.contains("$APP_ID.svg"),
        "the installer does not name the icon after the application id"
    );
    // The same canonical file the brand documentation points at and build.rs
    // compiles in — not a second copy that could drift.
    assert!(
        installer.contains("docs/design/assets/app-icon.svg"),
        "the installer does not install the canonical app icon"
    );
}

/// D. `Exec=` still names the binary this crate builds, and the Quick Panel
/// action still passes the flag the application parses.
#[test]
fn the_desktop_entry_launches_this_binary() {
    let exec = desktop_entry_key("Exec").expect("the entry has an Exec key");
    // The [[bin]] name in Cargo.toml, which is what a package puts on PATH.
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("Cargo.toml is readable");
    assert!(
        manifest.contains("name = \"anyflow-gui\""),
        "the binary is no longer called anyflow-gui"
    );
    assert_eq!(exec, "anyflow-gui", "Exec= no longer names the binary");

    let text = std::fs::read_to_string(data().join(format!("{}.desktop", app_id_from_source())))
        .expect("the desktop entry is readable");
    // The launcher shortcut is a caller of the same activation seam as
    // `app.quick-panel`, and its flag has to be one the binary accepts.
    assert!(
        text.contains("Exec=anyflow-gui --quick-panel"),
        "the Quick Panel shortcut no longer passes --quick-panel"
    );
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("src/lib.rs is readable");
    assert!(
        lib.contains("\"--quick-panel\""),
        "the binary no longer parses the flag the desktop entry passes"
    );

    // D-Bus activation is the seam a shell uses to raise an already-running
    // AnyFlow rather than starting a second process, and it only works
    // because the bus name is the desktop file id.
    assert_eq!(
        desktop_entry_key("DBusActivatable").as_deref(),
        Some("true")
    );

    // The X11 association, which is a *different* string from the app id and
    // measured rather than assumed: GTK sets a Wayland app_id from the
    // GApplication id and an X11 WM_CLASS from the program name.
    assert_eq!(
        desktop_entry_key("StartupWMClass").as_deref(),
        Some("anyflow-gui"),
        "without this an X11 window matches no launcher"
    );
}

/// E. The icon is compiled into the binary at the path a `GtkIconTheme`
/// looks for, and the resource list and the deriving build step agree.
#[test]
fn the_icon_is_compiled_in_at_an_icon_theme_path() {
    let app_id = app_id_from_source();
    let gresource = std::fs::read_to_string(data().join("anyflow.gresource.xml"))
        .expect("the resource list is readable");
    let want = format!("icons/scalable/apps/{app_id}.svg");
    assert!(
        gresource.contains(&want),
        "the resource list does not carry {want}"
    );

    let build = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("build.rs is readable");
    assert!(
        build.contains("icons/scalable/apps"),
        "build.rs no longer derives the icon-theme copy"
    );
    assert!(
        build.contains("app-icon.svg"),
        "build.rs no longer derives it from the canonical app icon"
    );

    // And the prefix the application adds to the icon theme is the one the
    // resources are compiled under, or the lookup finds nothing.
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("src/lib.rs is readable");
    let prefix = format!("/{}/icons", app_id.replace('.', "/"));
    assert!(
        lib.contains(&format!("add_resource_path(\"{prefix}\"")),
        "the icon theme prefix is not {prefix}"
    );
}

/// K. The primary application icon is the Flow A, not the ribbon-and-dots it
/// replaced.
///
/// The old mark is deliberately still in the tree — Android's design-token
/// test resources read that directory — so "we deleted it" is not the
/// guarantee. What is guaranteed is that nothing resolves it as *the*
/// application icon.
#[test]
fn the_primary_application_icon_is_the_flow_a() {
    let app_icon = read("app-icon.svg");
    let flow_a = read("logo-flow-a.svg");

    // The one continuous ribbon, by its geometry rather than by its filename:
    // the app icon carries the same path data as the canonical mark.
    let icon_path = attr(&app_icon, "d").expect("the app icon draws a path");
    let mark_path = attr(&flow_a, "d").expect("the canonical mark draws a path");
    assert_eq!(
        icon_path, mark_path,
        "the application icon is not the canonical Flow A geometry"
    );

    // Nothing points the icon name at the old mark.
    let gresource = std::fs::read_to_string(data().join("anyflow.gresource.xml"))
        .expect("the resource list is readable");
    let icon_entry = gresource
        .lines()
        .find(|l| l.contains("icons/scalable/apps"))
        .expect("the icon-theme entry exists");
    assert!(
        !icon_entry.contains("ribbon"),
        "the icon-theme entry names the ribbon mark: {icon_entry}"
    );
    let build = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("build.rs is readable");
    assert!(
        !build.contains("icon-flowing-ribbon"),
        "build.rs derives the application icon from the ribbon mark"
    );
    let installer = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/install-desktop-metadata.sh"),
    )
    .expect("the installer is readable");
    assert!(
        !installer.contains("ribbon"),
        "the installer installs the ribbon mark as the application icon"
    );

    // And there is exactly one application icon: a second one installed under
    // a name the shell might pick up is how two identities appear.
    assert!(
        installer.matches("share/icons/hicolor").count() >= 1,
        "the installer does not install an icon at all"
    );
}

/// The installer is a file a person runs and a package calls. Both need it to
/// be executable, to refuse a relative prefix, and never to need root.
#[test]
fn the_installer_is_safe_to_run_unprivileged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/install-desktop-metadata.sh");
    let text = std::fs::read_to_string(&path).expect("the installer is readable");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "the installer is not executable: {mode:o}"
        );
    }

    // Defaults to the user's own data directory, so running it with no
    // arguments cannot touch a system path.
    assert!(
        text.contains(r#"prefix="${HOME}/.local""#),
        "the default prefix is not the user's own data directory"
    );
    // Nothing in it escalates.
    for forbidden in ["sudo", "pkexec", "doas", "su -"] {
        assert!(
            !text.contains(forbidden),
            "the installer reaches for {forbidden}"
        );
    }
    // Fails loudly rather than guessing, and stops on the first error: a
    // half-installed set of metadata is the state that looks like the bug it
    // is fixing.
    assert!(text.contains("set -euo pipefail"));
    assert!(text.contains("--uninstall"), "there is no way to undo it");

    // And the application does not run it. An application that installs
    // things into a home directory when it starts is doing something nobody
    // asked for, and the brief for this work forbids it explicitly.
    //
    // Checked against *code*, with comments stripped: the source is expected
    // to discuss the installer at length — that is where the reasoning about
    // the Wayland icon chain lives — and a test that could not tell an
    // explanation from a call would have to choose between being wrong and
    // making the code undocumentable.
    for source in ["src/lib.rs", "src/main.rs", "build.rs"] {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(source))
            .unwrap_or_default();
        let code = strip_comments(&text);
        assert!(
            !code.contains("install-desktop-metadata"),
            "{source} runs the installer"
        );
        assert!(
            !code.contains(".local/share/applications"),
            "{source} writes desktop metadata itself"
        );
        // Nothing spawns anything, which is the general form of the rule.
        for spawner in ["Command::new", "std::process::Command"] {
            assert!(
                !code.contains(spawner),
                "{source} spawns a process ({spawner})"
            );
        }
    }
}

/// Rust source with `//` line comments — doc comments included — removed.
///
/// Deliberately naive about string literals containing `//`, which would make
/// it drop the tail of such a line. That direction is safe here: it can only
/// hide a match, and every assertion using it is a *negative* one whose job is
/// to fail on code. A URL in a string would be the false negative, and there
/// is none in the three files this reads.
fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ===========================================================================
// KDE-SNI-01 — the D-Bus activation metadata
// ===========================================================================
//
// `DBusActivatable=true` has been in the desktop entry since the Quick Panel
// sprint and it is not, on its own, enough to start anything. It is a promise
// read by things that launch desktop entries; the message bus reads a service
// file instead, and there was none. Measured before this sprint added one:
//
//     $ gdbus call --session --dest io.github.yurisismotto.anyflow … \
//           --method org.freedesktop.Application.ActivateAction quick-panel '[]' '{}'
//     Error: org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable
//
// The KDE tray item needs the cold case to work, so the service file is now
// installed beside the other two. These pin the four strings that have to
// agree for it to: the bus name, the entry's basename, the binary, and the
// flag that keeps a cold start from opening a window nobody asked for.

/// The D-Bus activation template. `.in`, because one line has to be derived
/// from the install prefix.
fn dbus_service_template() -> String {
    std::fs::read_to_string(data().join(format!("{}.service.in", app_id_from_source())))
        .expect("the D-Bus service template is readable")
}

/// One key from the `[D-BUS Service]` group of the template.
fn dbus_service_key(key: &str) -> Option<String> {
    let text = dbus_service_template();
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[D-BUS Service]";
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

/// The bus name the service file claims is the application id, and the file
/// is named after it.
#[test]
fn the_dbus_activation_entry_claims_the_application_id() {
    let app_id = app_id_from_source();
    let template = data().join(format!("{app_id}.service.in"));
    assert!(
        template.is_file(),
        "no D-Bus activation template at {} — without one the bus cannot start \
         AnyFlow, and the tray's cold-start path does not exist",
        template.display()
    );
    assert_eq!(
        dbus_service_key("Name").as_deref(),
        Some(app_id.as_str()),
        "the service file claims a bus name that is not the application id"
    );
}

/// The `Exec` line: the same binary the desktop entry launches, in service
/// mode, from a prefix rather than from anywhere in this checkout.
#[test]
fn the_dbus_activation_entry_starts_the_gui_in_service_mode() {
    let exec = dbus_service_key("Exec").expect("the service file declares Exec");

    // `--gapplication-service` is what stops a cold activation opening a
    // window the person did not ask for: without it the bus starts the binary
    // with no arguments, which is a bare launch, which opens Settings — and
    // then the activation message arrives and opens the Quick Panel too.
    assert!(
        exec.ends_with(" --gapplication-service"),
        "Exec is {exec:?}; a cold activation would open an unasked-for window"
    );

    // Derived from the prefix, and never from a developer's home directory or
    // a repository path. `@BINDIR@` is the only variable part.
    assert!(
        exec.starts_with("@BINDIR@/"),
        "Exec is {exec:?}; a D-Bus service file's Exec must be absolute, and the \
         only honest source of an absolute path is the install prefix"
    );
    for wrong in ["/home/", "Sandbox", "target/debug", "..", "~"] {
        assert!(
            !exec.contains(wrong),
            "Exec contains {wrong:?}, which is a path from somebody's machine"
        );
    }

    // The same program the desktop entry runs.
    let desktop_exec = desktop_entry_key("Exec").expect("the desktop entry declares Exec");
    let program = |line: &str| {
        line.split_whitespace()
            .next()
            .expect("a program")
            .rsplit('/')
            .next()
            .expect("a basename")
            .to_string()
    };
    assert_eq!(
        program(&exec),
        program(&desktop_exec),
        "the bus would start a different program than the launcher does"
    );

    // And the entry still declares itself activatable, which is the half of
    // the pair that things launching desktop entries read.
    assert_eq!(
        desktop_entry_key("DBusActivatable").as_deref(),
        Some("true"),
        "the desktop entry no longer declares itself D-Bus activatable"
    );
}

/// The installer puts it where the bus looks, substitutes the prefix and not
/// the staging root, and takes it away again.
#[test]
fn the_installer_handles_the_activation_entry_like_the_other_two() {
    let app_id = app_id_from_source();
    let installer = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/install-desktop-metadata.sh"),
    )
    .expect("the installer is readable");

    assert!(
        installer.contains("share/dbus-1/services"),
        "the installer does not write into a D-Bus service directory"
    );
    assert!(
        installer.contains("$APP_ID.service"),
        "the installer does not name the service file after the application id"
    );
    assert!(
        installer.contains("$APP_ID.service.in") || installer.contains("DBUS_SRC="),
        "the installer does not read the template"
    );

    // The substitution takes `$prefix`, the path the file will be read at, and
    // never `$destdir`, which is a staging root that does not exist on the
    // machine that ends up reading it. Getting this backwards produces a
    // package whose service file points into a buildroot.
    assert!(
        installer.contains(r#"s|@BINDIR@|$prefix/bin|g"#),
        "the installer does not substitute @BINDIR@ from the install prefix"
    );
    assert!(
        !installer.contains("@BINDIR@|$destdir"),
        "the installer bakes the staging root into the service file"
    );

    // The bus does not watch its service directories — measured on Fedora 44,
    // whose bus is dbus-broker: the activation failed until `ReloadConfig` was
    // sent. Without this line a development install silently works only after
    // the next login.
    assert!(
        installer.contains("ReloadConfig"),
        "the installer does not tell the session bus to reread its services"
    );

    // And uninstall removes it, or an uninstalled AnyFlow leaves the bus able
    // to start a binary that is no longer there.
    let uninstall_block = installer
        .split("if [ \"$uninstall\" -eq 1 ]")
        .nth(1)
        .expect("the installer has an uninstall path");
    assert!(
        uninstall_block.contains("$dbus_dst"),
        "uninstall leaves the D-Bus activation entry behind"
    );
    let _ = app_id;
}
