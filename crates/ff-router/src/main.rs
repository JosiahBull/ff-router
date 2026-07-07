//! A thin macOS "default browser" that routes clicked links to the right
//! Firefox profile based on globs in `~/.ff-router.toml`.

mod config;
mod glob;

use std::process::Command;

use config::Config;

const FIREFOX: &str = "/Applications/Firefox.app/Contents/MacOS/firefox";

fn main() {
    match std::env::args().nth(1).as_deref() {
        // Ask macOS to make us the default browser (triggers the system prompt).
        Some("--set-default") => macos::set_default_browser(),
        // Direct invocation with a URL (terminal use / testing) skips AppKit.
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => launch(url),
        _ => macos::run(),
    }
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
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSWorkspace};
    use objc2_foundation::{NSArray, NSBundle, NSError, NSString, NSURL};

    /// Ask macOS to make this app the default browser. Uses the async
    /// `NSWorkspace` API (macOS 12+), which shows the system "change your
    /// default web browser?" prompt. Setting the `http` handler is what defines
    /// the default browser and `https` follows it, so we set only `http`.
    ///
    /// The change is applied after the user responds, so we stay alive on the
    /// run loop and exit from the completion handler (setting both schemes at
    /// once or exiting early makes the change fail or revert).
    pub fn set_default_browser() {
        let mtm = MainThreadMarker::new().expect("main thread");
        let app = NSApplication::sharedApplication(mtm);
        let workspace = NSWorkspace::sharedWorkspace();
        let bundle_url = NSBundle::mainBundle().bundleURL();
        let scheme = NSString::from_str("http");

        let completion = RcBlock::new(|error: *mut NSError| {
            if let Some(error) = unsafe { error.as_ref() } {
                eprintln!(
                    "could not set default browser: {}",
                    error.localizedDescription()
                );
            } else {
                println!("Default browser updated.");
            }
            std::process::exit(0);
        });

        workspace.setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
            &bundle_url,
            &scheme,
            Some(&completion),
        );
        app.run();
    }

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
