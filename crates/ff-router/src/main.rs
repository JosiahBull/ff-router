//! A thin macOS "default browser" that routes clicked links to the right
//! Firefox profile based on globs in `~/.ff-router.toml`.
//!
//! When set as the default browser, macOS launches this app with the clicked
//! URL(s) via the `application:openURLs:` delegate callback. We match the URL
//! against the config, then re-launch Firefox with the chosen `--profile`.

mod config;

use std::process::Command;

use config::Config;

const FIREFOX: &str = "/Applications/Firefox.app/Contents/MacOS/firefox";

fn main() {
    // Direct invocation with a URL (terminal use / testing) skips AppKit.
    if let Some(url) = std::env::args().nth(1) {
        if url.starts_with("http://") || url.starts_with("https://") {
            launch(&url);
            return;
        }
    }
    macos::run();
}

/// Launch Firefox with the URL in the profile the config routes it to. Falls
/// back to Firefox's default profile when there is no config or no match.
fn launch(url: &str) {
    let mut cmd = Command::new(FIREFOX);
    if let Some(profile) = Config::load().and_then(|c| c.profile_path(url)) {
        cmd.arg("--profile").arg(profile);
    }
    cmd.arg(url);
    let _ = cmd.spawn();
}

mod macos {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSApplicationDelegate};
    use objc2_foundation::{NSArray, NSURL};

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "FFRouterDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl NSApplicationDelegate for Delegate {
            #[unsafe(method(application:openURLs:))]
            fn open_urls(&self, _app: &NSApplication, urls: &NSArray<NSURL>) {
                for url in urls {
                    if let Some(s) = url.absoluteString() {
                        super::launch(&s.to_string());
                    }
                }
                std::process::exit(0);
            }
        }
    );

    impl Delegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(mtm), init] }
        }
    }

    /// Run as a background app until macOS hands us a URL to route.
    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main thread");
        let app = NSApplication::sharedApplication(mtm);

        // The delegate must outlive `run()` (NSApplication holds it weakly).
        let delegate = Delegate::new(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        app.run();
    }
}
