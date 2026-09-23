//! The Wayland clipboard, via the `wl-clipboard` tools.
//!
//! # Why an external helper and not a library
//!
//! Reading and writing a Wayland clipboard is not a function call. A client
//! that *offers* the clipboard has to stay alive to serve every paste request
//! that follows, and a client that *reads* it needs a seat and an input
//! serial from the compositor. `wl-copy` already does the first correctly —
//! it forks and lives on as the selection owner — and doing it in-process
//! would mean running a Wayland event loop inside a headless daemon for no
//! gain. `wl-clipboard` is the standard implementation, is packaged
//! everywhere, and is what every other desktop integration on Linux uses.
//!
//! # How the child processes are driven, and why it matters
//!
//! Two properties of these tools shape every call below, and both were found
//! by running them rather than by reading them:
//!
//! * **`wl-copy` daemonises and inherits its parent's stdio.** A parent that
//!   hands it a pipe and then waits for EOF waits for ever, because the
//!   forked child holds the write end open for as long as it owns the
//!   clipboard — which is until the *next* copy, possibly hours later. Every
//!   spawn here therefore gives it `/dev/null` for stdout and stderr, and
//!   waits only on the short-lived parent.
//! * **These tools block, rather than fail, when the compositor will not
//!   serve them.** On a locked GNOME session `wl-copy` and `wl-paste` wait
//!   indefinitely for a seat that is never granted. Every invocation is
//!   therefore wrapped in [`BACKEND_TIMEOUT`] with `kill_on_drop`, so a
//!   locked screen costs one killed child rather than one leaked child per
//!   attempt, for ever.
//!
//! Nothing here builds a shell command. The text is written to the child's
//! **stdin** and never appears in an `argv`, so there is no quoting to get
//! wrong, nothing for a `$(…)` in a clipboard to expand into, and no
//! clipboard content in `/proc/<pid>/cmdline` where any process on the
//! machine could read it.

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{BackendError, BackendResult, ClipboardBackend, ClipboardWatch};
use crate::limits::BACKEND_TIMEOUT;
use crate::text::ClipboardText;

/// The binaries this backend needs.
const WL_COPY: &str = "wl-copy";
const WL_PASTE: &str = "wl-paste";

/// The MIME type used for both directions.
///
/// Explicit rather than inferred: `wl-copy` guesses a type from the content
/// otherwise, and a clip that happens to start with `<` should not become
/// `text/html` on the way to a phone.
const TEXT_MIME: &str = "text/plain;charset=utf-8";

/// Where clipboard-change notifications come from on this session.
#[derive(Debug, Clone)]
pub enum WatchSource {
    /// `wl-paste --watch`, which needs the compositor to implement
    /// `zwlr_data_control_manager_v1` or `ext_data_control_manager_v1`.
    /// sway, Hyprland and KWin do; Mutter does not.
    DataControl,
    /// XFIXES on the Xwayland `CLIPBOARD` selection. See
    /// [`super::x11`] and ADR-0014.
    X11Fixes,
    /// No event-driven source. The reason is shown in
    /// `omnibridge clipboard status`; the capability degrades to manual sending
    /// rather than polling.
    None(String),
}

impl WatchSource {
    fn describe(&self) -> String {
        match self {
            Self::DataControl => "wl-paste --watch (data-control)".to_string(),
            Self::X11Fixes => "XFIXES on the Xwayland CLIPBOARD selection".to_string(),
            Self::None(why) => format!("unavailable ({why})"),
        }
    }
}

/// Whether this system's `wl-copy` understands `--sensitive`.
///
/// Not a version check. Fedora ships `2.2.1^git20251124`, which *has* the
/// flag despite the version number, while Debian 13's and every current
/// Ubuntu LTS's `2.2.1` does not — so a `>= 2.3` dependency would exclude a
/// working system and admit a broken one. Ask the tool (PLAT-DEC-013).
#[derive(Debug, Clone)]
pub enum SensitiveSupport {
    /// `wl-copy --help` lists the option.
    Supported,
    /// It does not, and here is what to do about it.
    Unsupported(String),
}

impl SensitiveSupport {
    fn as_result(&self) -> Result<(), String> {
        match self {
            Self::Supported => Ok(()),
            Self::Unsupported(why) => Err(why.clone()),
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            Self::Supported => "yes",
            Self::Unsupported(_) => "no",
        }
    }
}

/// Clipboard access for a Wayland session.
pub struct WaylandBackend {
    /// `None` when the helper binaries are missing; carries why.
    tools: Result<(), String>,
    watch: WatchSource,
    sensitive: SensitiveSupport,
}

impl WaylandBackend {
    /// Probes the session once, at construction.
    ///
    /// Probing here rather than per call means `omnibridge clipboard status` can
    /// tell the user what will and will not work *before* they try it, and
    /// that the answer does not change under them mid-session.
    pub fn detect() -> Self {
        // Names the missing executables and the package that carries them,
        // and stops there. It used to end `(Fedora: sudo dnf install
        // wl-clipboard)`, which was the only string in the whole product that
        // named a distribution — and it is wrong for every user who is not on
        // one. OmniBridge does not know which package manager this machine has,
        // and guessing wrong is worse than not guessing: the package is called
        // `wl-clipboard` on Fedora, Ubuntu and Debian alike, so naming it once
        // is both shorter and true everywhere. Package-manager commands belong
        // in the documentation, per distribution, where they can be correct.
        let tools = match (which(WL_COPY), which(WL_PASTE)) {
            (true, true) => Ok(()),
            _ => Err(format!(
                "{WL_COPY}/{WL_PASTE} not found on PATH. Install the \
                 wl-clipboard package."
            )),
        };

        let watch = if tools.is_err() {
            WatchSource::None("wl-clipboard is not installed".to_string())
        } else {
            detect_watch_source()
        };

        let sensitive = if tools.is_err() {
            SensitiveSupport::Unsupported("wl-clipboard is not installed".to_string())
        } else {
            probe_sensitive()
        };

        tracing::info!(
            backend = "wl-clipboard",
            available = tools.is_ok(),
            watch = %watch.describe(),
            sensitive = sensitive.describe(),
            "clipboard backend"
        );

        Self {
            tools,
            watch,
            sensitive,
        }
    }

    /// Builds a backend with an explicit watch source. Tests only.
    #[doc(hidden)]
    pub fn with_watch_source(watch: WatchSource) -> Self {
        Self {
            tools: Ok(()),
            watch,
            sensitive: SensitiveSupport::Supported,
        }
    }

    /// Builds a backend with an explicit `--sensitive` verdict. Tests only.
    #[doc(hidden)]
    pub fn with_sensitive_support(sensitive: SensitiveSupport) -> Self {
        Self {
            tools: Ok(()),
            watch: WatchSource::None("not probed in this test".into()),
            sensitive,
        }
    }

    pub fn watch_source(&self) -> &WatchSource {
        &self.watch
    }

    pub fn sensitive_source(&self) -> &SensitiveSupport {
        &self.sensitive
    }

    /// Runs `wl-paste` once, for one MIME type.
    async fn read_as(&self, mime: &str) -> BackendResult<ReadAttempt> {
        let mut command = Command::new(WL_PASTE);
        command
            // `--no-newline`: wl-paste appends one otherwise, and a clipboard
            // that gains a trailing newline in transit is a clipboard that
            // corrupts what it carries.
            .arg("--no-newline")
            .arg("--type")
            .arg(mime)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let output = match tokio::time::timeout(BACKEND_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(BackendError::Failed(format!(
                    "could not run {WL_PASTE}: {e}"
                )))
            }
            Err(_) => return Err(BackendError::TimedOut),
        };

        if output.status.success() {
            // An empty clipboard can also exit 0 with no output.
            if output.stdout.is_empty() {
                return Ok(ReadAttempt::Nothing);
            }
            let Ok(text) = String::from_utf8(output.stdout) else {
                // Bytes that are not UTF-8 under a text offer. Nothing to
                // send, and not a fault worth alarming about.
                return Ok(ReadAttempt::Nothing);
            };
            return Ok(match ClipboardText::validate(text) {
                Ok(t) => ReadAttempt::Text(t),
                Err(rejection) => ReadAttempt::Rejected(rejection.to_string()),
            });
        }

        // A non-zero exit is ambiguous: an empty clipboard, a clipboard with
        // no offer of this type, and a broken compositor connection all land
        // here. The distinction matters — the first two mean "nothing to
        // send", the third is a fault worth reporting — so classify on stderr
        // rather than treating every failure as one or the other.
        //
        // The first two phrases below are wl-clipboard 2.2.1's exact wording,
        // taken from running it rather than guessed:
        //
        //   "Nothing is copied"
        //   "Clipboard content is not available as requested type \"…\""
        //
        // The broader ones around them are there so that a future rewording
        // degrades to "nothing" rather than to "the compositor is broken".
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("nothing is copied")
            || stderr.contains("not available as requested type")
            || stderr.contains("content is not available")
            || stderr.contains("no selection")
            || stderr.contains("selection is empty")
            || stderr.contains("no suitable type")
            || stderr.contains("offer")
        {
            return Ok(ReadAttempt::Nothing);
        }
        if stderr.contains("connect") || stderr.contains("compositor") || stderr.contains("wayland")
        {
            return Err(BackendError::Unavailable(format!(
                "{WL_PASTE} could not reach the Wayland compositor"
            )));
        }
        // Unclassified. Reported rather than swallowed, and without the
        // stdout that might have held clipboard content.
        Err(BackendError::Failed(format!(
            "{WL_PASTE} exited with {}",
            output.status
        )))
    }

    fn require_tools(&self) -> BackendResult<()> {
        self.tools.clone().map_err(BackendError::Unavailable)
    }
}

#[async_trait::async_trait]
impl ClipboardBackend for WaylandBackend {
    fn id(&self) -> &'static str {
        "wl-clipboard"
    }

    async fn read_text(&self) -> BackendResult<Option<ClipboardText>> {
        self.require_tools()?;

        // Two MIME types, in order, and both are needed.
        //
        // `wl-copy` offers `text/plain` *and* `text/plain;charset=utf-8`, so
        // either works against a clip OmniBridge itself wrote. Other
        // applications are not so obliging: some offer only the
        // charset-qualified form, and asking for bare `text/plain` against
        // one of those fails with "Clipboard content is not available as
        // requested type" — which looks exactly like an empty clipboard and
        // is not. Observed on GNOME, reading a clip written by another app.
        //
        // Asking for the qualified type first and falling back costs a second
        // process only when the first genuinely has nothing, and it stays an
        // explicit allow-list: omitting `--type` altogether would let
        // wl-paste hand back image bytes for an image clipboard.
        for (index, mime) in [TEXT_MIME, "text/plain"].iter().enumerate() {
            match self.read_as(mime).await? {
                ReadAttempt::Text(text) => return Ok(Some(text)),
                // There is text, but it is not carriable — oversized, or
                // NUL-bearing. A definite answer, not a reason to try the
                // other type.
                ReadAttempt::Rejected(why) => return Err(BackendError::Failed(why)),
                ReadAttempt::Nothing if index == 0 => continue,
                ReadAttempt::Nothing => return Ok(None),
            }
        }
        Ok(None)
    }

    async fn write_text(&self, text: &ClipboardText, sensitive: bool) -> BackendResult<()> {
        self.require_tools()?;

        let mut command = Command::new(WL_COPY);
        command.arg("--type").arg(TEXT_MIME);
        if sensitive {
            // The clip is asked to be marked, and this system may not be able
            // to mark it. Refuse, and say why.
            //
            // The alternative — write it without the flag and warn — was
            // considered and rejected. A `sensitive_hint` clip is, by
            // construction, the one the user least wants persisted in a
            // clipboard-history manager, so silently downgrading turns a
            // visible failure into an invisible privacy regression. Refusing
            // is fail-closed, which is the right direction; what was wrong
            // before Wave 0 was only that nothing explained it
            // (PLAT-DEC-013).
            self.sensitive.as_result().map_err(|why| {
                BackendError::Unavailable(format!(
                    "this clip is marked sensitive and {why} It was NOT written \
                     to the clipboard: writing it unmarked would leave a \
                     password in your clipboard manager's history without \
                     telling you."
                ))
            })?;
            // Asks clipboard managers not to keep this in their history. A
            // hint to the desktop, exactly as EXTRA_IS_SENSITIVE is on
            // Android — not enforcement, and nothing depends on it.
            command.arg("--sensitive");
        }
        command
            .stdin(Stdio::piped())
            // Both null, and this is the load-bearing part: wl-copy forks and
            // the survivor holds whatever it was given until the clipboard
            // changes again. Handing it a pipe would hang any parent that
            // later waited on that pipe.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let write = async {
            let mut child = command
                .spawn()
                .map_err(|e| BackendError::Failed(format!("could not run {WL_COPY}: {e}")))?;

            {
                let mut stdin = child.stdin.take().ok_or_else(|| {
                    BackendError::Failed(format!("{WL_COPY} did not provide a stdin"))
                })?;
                stdin
                    .write_all(text.as_str().as_bytes())
                    .await
                    .map_err(|e| {
                        BackendError::Failed(format!("could not write to {WL_COPY}: {e}"))
                    })?;
                stdin.flush().await.ok();
                // Dropping stdin closes it, which is what tells wl-copy the
                // content is complete. Without this it waits for EOF and the
                // clipboard is never set.
            }

            let status = child
                .wait()
                .await
                .map_err(|e| BackendError::Failed(format!("{WL_COPY} did not exit: {e}")))?;

            if status.success() {
                Ok(())
            } else {
                Err(BackendError::Failed(format!(
                    "{WL_COPY} exited with {status}"
                )))
            }
        };

        match tokio::time::timeout(BACKEND_TIMEOUT, write).await {
            Ok(result) => result,
            Err(_) => Err(BackendError::TimedOut),
        }
    }

    fn watch_changes(&self) -> BackendResult<ClipboardWatch> {
        self.require_tools()?;
        match &self.watch {
            WatchSource::DataControl => spawn_wl_paste_watch(),
            WatchSource::X11Fixes => super::x11::watch_clipboard(),
            WatchSource::None(why) => Err(BackendError::Unavailable(why.clone())),
        }
    }

    fn watch_availability(&self) -> std::result::Result<(), String> {
        self.tools.clone()?;
        match &self.watch {
            WatchSource::DataControl | WatchSource::X11Fixes => Ok(()),
            WatchSource::None(why) => Err(why.clone()),
        }
    }

    /// The ordinary clipboard, which on this backend means: are the two
    /// helper binaries here. Nothing about `--sensitive` enters this answer —
    /// every current Ubuntu LTS and Debian Stable is precisely the case where
    /// the first is true and the second is not.
    fn availability(&self) -> std::result::Result<(), String> {
        self.tools.clone()
    }

    fn sensitive_support(&self) -> std::result::Result<(), String> {
        self.tools.clone()?;
        self.sensitive.as_result()
    }

    fn describe(&self) -> String {
        match &self.tools {
            Ok(()) => format!(
                "wl-clipboard; watch: {}; sensitive marking: {}",
                self.watch.describe(),
                self.sensitive.describe()
            ),
            Err(why) => format!("wl-clipboard unavailable ({why})"),
        }
    }
}

/// One outcome of asking `wl-paste` for a particular MIME type.
enum ReadAttempt {
    Text(ClipboardText),
    /// There is text, but it cannot be carried.
    Rejected(String),
    /// Nothing is available as this type.
    Nothing,
}

/// Whether a binary is on `PATH`.
///
/// Deliberately not `which(1)`: spawning a process to find out whether we can
/// spawn a process is silly, and `PATH` resolution is four lines.
fn which(binary: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(binary).is_file())
}

/// Decides where clipboard-change notifications will come from.
fn detect_watch_source() -> WatchSource {
    match probe_data_control() {
        Ok(()) => return WatchSource::DataControl,
        Err(why) => {
            tracing::info!(
                reason = %why,
                "wl-paste --watch is unavailable; falling back to the X11 bridge"
            );
        }
    }

    match super::x11::probe() {
        Ok(()) => WatchSource::X11Fixes,
        // NOT "manual send still works". It does not, and saying so was
        // finding F-2.
        //
        // Both paths read a selection this process does not own: the watcher
        // needs it continuously, `clipboard send` needs it once. Neither
        // data-control nor the Xwayland bridge is available in this branch, so
        // `wl-paste` waits for a seat the compositor will not grant and the
        // read ends in BackendError::TimedOut. Measured on Debian 13 trixie
        // with GNOME 48 Wayland, the session provably unlocked.
        //
        // Receiving is genuinely unaffected, and that is worth saying, because
        // it is the half that still works: writing a clip with `wl-copy` needs
        // no data-control protocol.
        Err(why) => WatchSource::None(format!(
            "the compositor does not implement the wlr/ext data-control \
             protocol, and the Xwayland fallback is not usable either ({why}). \
             This session cannot read a selection it does not own, so neither \
             automatic nor manual sending can work here. Receiving a clipboard \
             from a paired device is unaffected."
        )),
    }
}

/// Asks `wl-copy` whether it understands `--sensitive`.
///
/// `wl-copy --help` prints its option list and needs no compositor, so this
/// is cheap, side-effect-free and safe behind a lock screen — unlike actually
/// running `wl-copy --sensitive`, which would set the clipboard.
///
/// The alternative, parsing `--version`, is refuted by the evidence: Fedora's
/// `2.2.1^git20251124` has the flag and Debian's `2.2.1` does not.
fn probe_sensitive() -> SensitiveSupport {
    use std::process::Command as SyncCommand;

    let output = SyncCommand::new(WL_COPY)
        .arg("--help")
        .stdin(Stdio::null())
        .output();

    let text = output.ok().map(|out| {
        // Merged, because some builds print usage on stderr and some on
        // stdout, and which one is not the question being asked.
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        text
    });
    probe_sensitive_from_output(text)
}

/// The verdict, given whatever `wl-copy --help` printed.
///
/// Split out so the decision is testable without a `wl-copy` of either
/// vintage on the machine running the tests — which matters, because the
/// development machine is precisely the one where this cannot be reproduced.
fn probe_sensitive_from_output(help: Option<String>) -> SensitiveSupport {
    // Distro-neutral, and deliberately careful about the version number.
    // `--sensitive` appeared in upstream wl-clipboard 2.3.0, which is worth
    // telling the user — but the number is guidance for choosing a build, not
    // the test OmniBridge applies, and the wording must not imply otherwise:
    // Fedora's `2.2.1^git20251124` carries a backport of the flag and passes
    // this probe, while Ubuntu's and Debian's plain 2.2.1 do not. The probe
    // above is the authority. No package-manager command appears here.
    const REMEDY: &str = "this system\'s wl-copy does not support sensitive \
         clipboard marking. Install a wl-clipboard build whose wl-copy \
         accepts `--sensitive` (upstream added it in 2.3.0; some \
         distributions backport it into an earlier version).";

    match help {
        Some(text) if text.contains("--sensitive") => SensitiveSupport::Supported,
        Some(_) => SensitiveSupport::Unsupported(REMEDY.to_string()),
        // Could not run it at all. Being unable to prove support is not the
        // same as proving it, so the safe answer is "no".
        None => {
            SensitiveSupport::Unsupported(format!("{REMEDY} (wl-copy could not be run to check)"))
        }
    }
}

/// Starts `wl-paste --watch` briefly to find out whether the compositor
/// supports it.
///
/// There is no way to ask a compositor "do you implement data-control"
/// without a Wayland connection of our own, so we ask the tool that already
/// knows. It fails immediately and loudly when the protocol is missing, and
/// stays alive for ever when it is present — so "still running after a
/// moment" is the answer.
fn probe_data_control() -> Result<(), String> {
    use std::process::Command as SyncCommand;

    let mut child = SyncCommand::new(WL_PASTE)
        .arg("--watch")
        // `true(1)` consumes nothing and exits 0. The probe only cares
        // whether wl-paste itself survives; what it would have run is
        // irrelevant.
        .arg("/usr/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run {WL_PASTE}: {e}"))?;

    // Long enough for the compositor round-trip that discovers the globals,
    // short enough not to delay daemon startup noticeably.
    std::thread::sleep(std::time::Duration::from_millis(400));

    match child.try_wait() {
        // Exited already: the protocol is missing (wl-paste prints
        // "Watch mode requires a compositor that supports the data-control
        // protocol" and exits non-zero).
        Ok(Some(status)) => Err(format!("wl-paste --watch exited with {status}")),
        // Still running, so the compositor served it.
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            Ok(())
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("could not probe {WL_PASTE}: {e}"))
        }
    }
}

/// Runs `wl-paste --watch` and turns its output into change signals.
///
/// The command it runs is `/usr/bin/echo`, which ignores its stdin and prints
/// exactly one newline. That is the whole trick: each clipboard change
/// produces one unambiguous byte on our pipe, so the framing is trivial and —
/// far more importantly — **the clipboard content never enters this pipe at
/// all**. The obvious alternative, `wl-paste --watch cat`, would stream every
/// clip through our stdout with no delimiter, making the framing ambiguous
/// for any clip containing a newline *and* putting content somewhere it does
/// not need to be. The manager reads the clipboard itself, once, when it is
/// ready to.
fn spawn_wl_paste_watch() -> BackendResult<ClipboardWatch> {
    let mut child = Command::new(WL_PASTE)
        .arg("--watch")
        .arg("/usr/bin/echo")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| BackendError::Failed(format!("could not start {WL_PASTE} --watch: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BackendError::Failed(format!("{WL_PASTE} did not provide a stdout")))?;

    let (tx, rx) = tokio::sync::mpsc::channel(64);

    let task = tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(_)) = lines.next_line().await {
            // A full queue means the manager has not caught up. Dropping the
            // signal is correct rather than lossy: the manager reads the
            // clipboard's *current* state when it does catch up, so one
            // signal is as good as five.
            if tx.try_send(()).is_err() && tx.is_closed() {
                break;
            }
        }
        // Reap the child so it does not linger as a zombie.
        let _ = child.wait().await;
    });

    Ok(ClipboardWatch::new(
        rx,
        "wl-paste --watch",
        Box::new(AbortOnDrop(task)),
    ))
}

/// Stops the reader task — and with it the child, via `kill_on_drop` — when
/// the watch is dropped.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_a_real_binary_and_rejects_a_fake_one() {
        // `sh` exists on every system this daemon runs on. (We never execute
        // it — this only exercises PATH resolution.)
        assert!(which("sh"));
        assert!(!which("omnibridge-definitely-not-a-real-binary"));
    }

    #[tokio::test]
    async fn a_backend_without_tools_reports_unavailable_rather_than_failing_late() {
        let backend = WaylandBackend {
            tools: Err("wl-clipboard is not installed".into()),
            watch: WatchSource::None("no tools".into()),
            sensitive: SensitiveSupport::Unsupported("no tools".into()),
        };
        assert!(matches!(
            backend.read_text().await,
            Err(BackendError::Unavailable(_))
        ));
        let text = ClipboardText::validate("x").expect("valid");
        assert!(matches!(
            backend.write_text(&text, false).await,
            Err(BackendError::Unavailable(_))
        ));
        assert!(matches!(
            backend.watch_changes(),
            Err(BackendError::Unavailable(_))
        ));
        assert!(backend.describe().contains("unavailable"));
    }

    // -----------------------------------------------------------------
    // PLAT-DEC-013 — `wl-copy --sensitive` compatibility
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn a_backend_that_can_mark_sensitive_writes_the_flag_and_says_so() {
        let backend = WaylandBackend::with_sensitive_support(SensitiveSupport::Supported);
        assert_eq!(backend.sensitive_support(), Ok(()));
        assert!(backend.describe().contains("sensitive marking: yes"));
    }

    #[tokio::test]
    async fn an_old_wl_copy_refuses_a_sensitive_clip_and_names_the_remedy() {
        // wl-clipboard 2.2.1 — Debian 13 and every current Ubuntu LTS. Its
        // unknown-option path is `print_usage(stderr); exit(1)`, so before
        // Wave 0 this produced an opaque "wl-copy exited with 1" for exactly
        // the class of clip that carries passwords.
        let backend = WaylandBackend::with_sensitive_support(SensitiveSupport::Unsupported(
            "this system\'s wl-copy does not support marking a clip sensitive. \
             `--sensitive` was added in wl-clipboard 2.3.0; upgrade the \
             wl-clipboard package to honour sensitive clips."
                .into(),
        ));

        let text = ClipboardText::validate("hunter2").expect("valid");
        let err = backend
            .write_text(&text, true)
            .await
            .expect_err("a sensitive clip must not be written unmarked");

        // Fail-closed, and explained: cause and remedy, no clipboard content.
        let message = err.to_string();
        assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");
        assert!(message.contains("wl-clipboard 2.3.0"), "{message}");
        assert!(message.contains("NOT written"), "{message}");
        assert!(!message.contains("hunter2"), "no content in an error");
    }

    #[test]
    fn an_old_wl_copy_does_not_claim_it_can_mark_a_clip() {
        // The capability must not lie about what it supports: `omnibridge
        // clipboard status` has to be able to tell the user before they turn
        // anything on.
        let backend = WaylandBackend::with_sensitive_support(SensitiveSupport::Unsupported(
            "wl-clipboard 2.3.0 needed".into(),
        ));
        assert!(backend.sensitive_support().is_err());
        assert!(backend.describe().contains("sensitive marking: no"));
    }

    #[test]
    fn an_unrunnable_wl_copy_is_reported_as_unsupported_not_as_supported() {
        // "Could not prove support" is not "supported". The probe fails
        // closed, in the same direction as the write path.
        match probe_sensitive_from_output(None) {
            SensitiveSupport::Unsupported(why) => assert!(why.contains("2.3.0"), "{why}"),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn the_probe_reads_the_option_list_rather_than_a_version_number() {
        // wl-clipboard 2.2.1\u2019s real option list, from its `long_options`
        // table. No `sensitive` entry.
        let old = "Usage: wl-copy [options] text to copy\n\
                   \t-o, --paste-once\tOnly serve one paste request.\n\
                   \t-f, --foreground\tStay in the foreground.\n\
                   \t-c, --clear\tInstead of copying, clear the clipboard.\n\
                   \t-p, --primary\tUse the primary selection.\n\
                   \t-n, --trim-newline\tDo not copy the trailing newline.\n\
                   \t-t, --type mime/type\tSet the MIME type.\n\
                   \t-s, --seat seat-name\tPick the seat.\n";
        assert!(matches!(
            probe_sensitive_from_output(Some(old.to_string())),
            SensitiveSupport::Unsupported(_)
        ));

        // 2.3.0 gained the option. Fedora\u2019s `2.2.1^git20251124` snapshot
        // also has it, which is precisely why the version string is the wrong
        // thing to look at.
        let new = format!("{old}\t--sensitive\tMark the clipboard content as sensitive.\n");
        assert!(matches!(
            probe_sensitive_from_output(Some(new)),
            SensitiveSupport::Supported
        ));
    }

    #[tokio::test]
    async fn an_ordinary_clip_is_unaffected_by_the_sensitive_verdict() {
        // The regression that matters most: nothing about a normal write may
        // change because this system\u2019s wl-copy is old. Proved without a
        // compositor by checking that the refusal is reached only for
        // `sensitive = true`; the write path beyond it is identical.
        let backend = WaylandBackend::with_sensitive_support(SensitiveSupport::Unsupported(
            "old wl-copy".into(),
        ));
        let text = ClipboardText::validate("ordinary").expect("valid");

        assert!(
            backend.write_text(&text, true).await.is_err(),
            "a sensitive clip must be refused"
        );

        // A non-sensitive write does not consult the verdict at all. It still
        // needs a real compositor to succeed, so what is asserted is that the
        // failure is *not* the sensitive refusal.
        if let Err(e) = backend.write_text(&text, false).await {
            assert!(
                !e.to_string().contains("sensitive"),
                "an ordinary write must not be refused for the sensitive \
                 reason, got: {e}"
            );
        }
    }

    #[test]
    fn a_missing_watch_source_is_described_without_pretending_to_work() {
        let backend = WaylandBackend::with_watch_source(WatchSource::None("mutter".into()));
        assert!(matches!(
            backend.watch_changes(),
            Err(BackendError::Unavailable(_))
        ));
        assert!(backend.describe().contains("unavailable"));
    }
}
