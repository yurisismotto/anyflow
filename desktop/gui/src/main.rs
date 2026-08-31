//! Entry point.
//!
//! Everything lives in the library beside this file so that the design
//! tokens, the widget vocabulary and the control client are a public,
//! testable surface rather than private details of a binary.

fn main() -> gtk::glib::ExitCode {
    anyflow_gui::run()
}
