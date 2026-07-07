//! A thin macOS "default browser" that routes clicked links to the right
//! Firefox profile based on globs in `~/.ff-router.toml`.

mod config;
mod glob;

use std::process::Command;

use config::Config;

const FIREFOX: &str = "/Applications/Firefox.app/Contents/MacOS/firefox";

/// Bundle identifier declared in the app's Info.plist.
const BUNDLE_ID: &str = "com.josiahbull.ff-router";

fn main() {
    match std::env::args().nth(1).as_deref() {
        // Ask macOS to make us the default browser (triggers the system prompt).
        Some("--set-default") => macos::set_default_browser(BUNDLE_ID),
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
    use std::ffi::c_void;

    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSApplicationDelegate};
    use objc2_foundation::{NSArray, NSString, NSURL};

    // LaunchServices C API. Setting the http/https handler for a web browser
    // prompts the user with the system "change default browser?" dialog. It is
    // deprecated but still functional (the same call the `defaultbrowser` CLI
    // uses); `NSString` is toll-free bridged to the expected `CFStringRef`.
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSSetDefaultHandlerForURLScheme(scheme: *const c_void, bundle_id: *const c_void) -> i32;
    }

    /// Ask macOS to make the app with `bundle_id` the default browser.
    pub fn set_default_browser(bundle_id: &str) {
        let bundle = NSString::from_str(bundle_id);
        for scheme in ["http", "https"] {
            let scheme_str = NSString::from_str(scheme);
            let status = unsafe {
                LSSetDefaultHandlerForURLScheme(
                    Retained::as_ptr(&scheme_str).cast::<c_void>(),
                    Retained::as_ptr(&bundle).cast::<c_void>(),
                )
            };
            if status != 0 {
                eprintln!("could not set the {scheme} handler (LaunchServices status {status})");
            }
        }
        println!("Requested default-browser change — confirm the macOS prompt if it appears.");
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
