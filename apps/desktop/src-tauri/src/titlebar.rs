//! The title bar macOS shows over a full-screen window when the pointer
//! reaches the top of the screen: by default an opaque white strip with the
//! window's buttons. In full screen PWR makes it glass -- Liquid Glass where
//! the system has it (macOS 26), a frosted blur before -- so the app shows
//! through it. In a window nothing changes: the web page draws its own top.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowButton,
    NSWindowOrderingMode,
};
use tauri::{Runtime, WebviewWindow};

/// What was changed for full screen, to be put back when it ends.
#[derive(Default)]
struct Glazed {
    /// The glass laid behind the window's buttons.
    glass: Option<Retained<NSView>>,
    /// The title bar's own backgrounds, hidden while the glass shows.
    hidden: Vec<Retained<NSView>>,
    /// The window macOS moves the title bar into, made see-through, with
    /// what it was.
    host: Option<(Retained<NSWindow>, Option<Retained<NSColor>>, bool)>,
}

thread_local! {
    // AppKit's views live on the main thread, and so does this.
    static GLAZED: RefCell<Glazed> = RefCell::new(Glazed::default());
}

/// Follows the window in and out of full screen. Called on its resizes and
/// focus changes; each call puts the title bar in the state the window is in,
/// so calling it again changes nothing.
pub fn follow<R: Runtime>(window: &WebviewWindow<R>) {
    let full = window.is_fullscreen().unwrap_or(false);
    let Ok(pointer) = window.ns_window() else { return };
    let pointer = pointer as usize;
    let _ = window.run_on_main_thread(move || {
        // SAFETY: the pointer is this window's NSWindow, alive while the
        // window is, and it is used on the main thread.
        let ns_window = unsafe { &*(pointer as *const NSWindow) };
        if full {
            glaze(ns_window);
        } else {
            clear(ns_window);
        }
    });
    // Entering full screen, macOS moves the title bar into its own window
    // only once the animation has run: look again when it has.
    if full {
        let window = window.clone();
        std::thread::spawn(move || {
            for wait in [400, 1200] {
                std::thread::sleep(std::time::Duration::from_millis(wait));
                let Ok(pointer) = window.ns_window() else { return };
                let pointer = pointer as usize;
                let _ = window.run_on_main_thread(move || {
                    let ns_window = unsafe { &*(pointer as *const NSWindow) };
                    glaze(ns_window);
                });
            }
        });
    }
}

/// The title bar's view and the container it sits in: found from the close
/// button, wherever macOS has put them.
fn title_bar(window: &NSWindow) -> Option<(Retained<NSView>, Retained<NSView>)> {
    let close = window.standardWindowButton(NSWindowButton::CloseButton)?;
    // SAFETY: plain view-tree reads on the main thread.
    let bar = unsafe { close.superview() }?;
    let container = unsafe { bar.superview() }?;
    Some((bar, container))
}

/// A view the system draws a title bar's background with.
fn is_background(view: &NSView) -> bool {
    let name = view.class().name().to_string_lossy();
    name.contains("Background") || name == "NSVisualEffectView" || name == "NSGlassEffectView"
}

fn glaze(window: &NSWindow) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some((bar, container)) = title_bar(window) else { return };
    GLAZED.with(|glazed| {
        let mut glazed = glazed.borrow_mut();
        let ours = glazed.glass.clone();
        let is_ours = |view: &NSView| ours.as_deref().is_some_and(|glass| std::ptr::eq(glass, view));
        for parent in [&bar, &container] {
            for view in parent.subviews().iter() {
                if is_ours(&view) || view.isHidden() || !is_background(&view) {
                    continue;
                }
                view.setHidden(true);
                glazed.hidden.push(view);
            }
        }
        if glazed.glass.is_none() {
            let glass = make_glass(mtm, container.bounds());
            glass.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            container.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
            glazed.glass = Some(glass);
        }
        // Once the title bar is in a window of its own, that window is made
        // see-through; the app's own window is never touched.
        if glazed.host.is_none() {
            if let Some(host) = container.window() {
                if !std::ptr::eq(&*host, window) {
                    let was = (host.backgroundColor(), host.isOpaque());
                    host.setOpaque(false);
                    host.setBackgroundColor(Some(&NSColor::clearColor()));
                    glazed.host = Some((host, Some(was.0), was.1));
                }
            }
        }
    });
}

fn clear(_window: &NSWindow) {
    GLAZED.with(|glazed| {
        let mut glazed = glazed.borrow_mut();
        if let Some(glass) = glazed.glass.take() {
            glass.removeFromSuperview();
        }
        for view in glazed.hidden.drain(..) {
            view.setHidden(false);
        }
        if let Some((host, colour, opaque)) = glazed.host.take() {
            host.setBackgroundColor(colour.as_deref());
            host.setOpaque(opaque);
        }
    });
}

/// Liquid Glass on macOS 26 and later; before, a frosted blur of what is
/// behind the title bar's window -- the app.
fn make_glass(mtm: MainThreadMarker, frame: objc2_foundation::NSRect) -> Retained<NSView> {
    if objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_some() {
        let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
        glass.setCornerRadius(0.0);
        return Retained::into_super(glass);
    }
    let blur = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
    blur.setMaterial(NSVisualEffectMaterial::HeaderView);
    blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    blur.setState(NSVisualEffectState::Active);
    Retained::into_super(blur)
}
