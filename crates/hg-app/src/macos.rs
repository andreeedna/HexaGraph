//! macOS: a file opened from the Finder (double-click on a .hxn) arrives as
//! an Apple event, not on the command line. An application delegate collects
//! the paths; the app opens them in its next frame.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationWillFinishLaunchingNotification};
use objc2_foundation::{MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSURL};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::{Mutex, OnceLock};

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Set once the app runs, to wake it up when a file arrives.
pub static CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

declare_class!(
    struct OpenDelegate;

    unsafe impl ClassType for OpenDelegate {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "HexaGraphOpenDelegate";
    }

    impl DeclaredClass for OpenDelegate {}

    unsafe impl NSObjectProtocol for OpenDelegate {}

    unsafe impl NSApplicationDelegate for OpenDelegate {
        #[method(application:openURLs:)]
        fn application_open_urls(&self, _app: &NSApplication, urls: &NSArray<NSURL>) {
            let paths = urls.iter().filter_map(|u| unsafe { u.path() }).map(|p| PathBuf::from(p.to_string()));
            OPENED.lock().unwrap().extend(paths);
            if let Some(ctx) = CTX.get() {
                ctx.request_repaint();
            }
        }
    }
);

/// Installs the delegate when the application is about to finish launching:
/// winit creates the application first, and the file of the double-click
/// that started the app arrives right after this moment.
pub fn install() {
    let block = block2::RcBlock::new(|_: NonNull<NSNotification>| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let delegate: Retained<OpenDelegate> = unsafe { msg_send_id![super(mtm.alloc().set_ivars(())), init] };
        NSApplication::sharedApplication(mtm).setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The application keeps only a weak reference: the delegate lives as long as the app.
        std::mem::forget(delegate);
    });
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSApplicationWillFinishLaunchingNotification),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(observer);
}

/// Files opened from the Finder since the last call.
pub fn take_opened() -> Vec<PathBuf> {
    std::mem::take(&mut *OPENED.lock().unwrap())
}
