//! The macOS open-file dialog (`NSOpenPanel`) for documents, shown as a non-blocking sheet:
//! the result comes back through a channel the app polls every frame. The MCP agent cannot
//! see into this system dialog, so the in-app path dialog (Cmd+L, the `open` tool) stays the
//! agent's way in. Same shape as the player's picker.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
#[cfg(target_os = "macos")]
use objc2_foundation::{ns_string, NSArray, NSString, NSURL};
#[cfg(target_os = "macos")]
use objc2_uniform_type_identifiers::UTType;

pub struct FilePicker {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    tx: Sender<Option<PathBuf>>,
    rx: Receiver<Option<PathBuf>>,
    open: bool,
}

impl Default for FilePicker {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            open: false,
        }
    }
}

impl FilePicker {
    /// Show the panel (one `.json` document). Only one panel at a time; off the main thread
    /// nothing happens and `false` is returned (the caller falls back to the in-app dialog).
    /// Off macOS there is no native panel: always `false`, same fallback.
    #[cfg(not(target_os = "macos"))]
    pub fn show(&mut self, _start_dir: Option<&std::path::Path>) -> bool {
        self.open
    }

    /// Show the panel (one `.json` document). Only one panel at a time; off the main thread
    /// nothing happens and `false` is returned (the caller falls back to the in-app dialog).
    #[cfg(target_os = "macos")]
    pub fn show(&mut self, start_dir: Option<&std::path::Path>) -> bool {
        if self.open {
            return true;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setAllowsMultipleSelection(false);
        panel.setCanChooseDirectories(false);
        panel.setCanChooseFiles(true);
        panel.setMessage(Some(ns_string!("Open a FUIDE CAD document (*.cad.json)")));
        panel.setPrompt(Some(ns_string!("Open")));
        if let Some(json) = UTType::typeWithFilenameExtension(ns_string!("json")) {
            panel.setAllowedContentTypes(&NSArray::from_slice(&[&*json]));
        }
        if let Some(dir) = start_dir {
            let url = NSURL::fileURLWithPath_isDirectory(
                &NSString::from_str(&dir.to_string_lossy()),
                true,
            );
            panel.setDirectoryURL(Some(&url));
        }
        let tx = self.tx.clone();
        let result_panel = panel.clone();
        let handler = block2::RcBlock::new(move |response: objc2_app_kit::NSModalResponse| {
            let path = if response == NSModalResponseOK {
                result_panel
                    .URLs()
                    .iter()
                    .filter_map(|u| u.path().map(|p| PathBuf::from(p.to_string())))
                    .next()
            } else {
                None
            };
            let _ = tx.send(path);
        });
        // the block is retained by AppKit for the panel session
        panel.beginWithCompletionHandler(&handler);
        self.open = true;
        true
    }

    /// The chosen file once the panel closed (`Some(None)` = cancelled).
    pub fn poll(&mut self) -> Option<Option<PathBuf>> {
        let r = self.rx.try_recv().ok();
        if r.is_some() {
            self.open = false;
        }
        r
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}
