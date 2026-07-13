//! A thin macOS "default browser" that routes clicked links to the right
//! Firefox profile based on globs in `~/.ff-router.toml`.

mod config;
mod debug;

use std::process::Command;

use config::{Config, Opener};

const FIREFOX: &str = "/Applications/Firefox.app/Contents/MacOS/firefox";

fn main() {
    match std::env::args().nth(1).as_deref() {
        // Ask macOS to make us the default browser (triggers the system prompt).
        Some("--set-default") => macos::set_default_browser(),
        // Direct invocation with a URL (terminal use / testing) skips AppKit,
        // so there is no opening application to attribute it to.
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => launch(url, None),
        _ => macos::run(),
    }
}

/// Launch Firefox with the URL in the profile the config routes it to, given
/// the application that opened it (`None` if unknown). Falls back to Firefox's
/// default profile when there is no config or no match. When the config has
/// `debug = true`, appends the decision to `~/.ff-router.log`.
fn launch(url: &str, opener: Option<&Opener>) {
    let config = Config::load();
    let profile = match &config {
        // Debug on: resolve *and* explain, then log the decision.
        Some(c) if c.is_debug() => {
            let decision = c.decide(url, opener);
            debug::log(url, opener, &decision.explanation);
            decision.profile
        }
        Some(c) => c.profile_path(url, opener),
        None => None,
    };

    let mut cmd = Command::new(FIREFOX);
    if let Some(profile) = profile {
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
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSRunningApplication,
        NSWorkspace,
    };
    use objc2_foundation::{
        NSAppleEventDescriptor, NSAppleEventManager, NSArray, NSBundle, NSError, NSString, NSURL,
    };

    use super::Opener;

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
            fn open_urls(&self, app: &NSApplication, urls: &NSArray<NSURL>) {
                // One Apple Event delivers the whole batch, so the opener is
                // shared across these URLs (in practice there is only one).
                let opener = current_opener();
                for url in urls {
                    if let Some(s) = url.absoluteString() {
                        super::launch(&s.to_string(), opener.as_ref());
                    }
                }
                // Launch Services *activates* us to deliver the open, which
                // re-promotes the process to a regular (Foreground) app — Dock
                // icon + Cmd+Tab entry — silently undoing the Accessory policy
                // set in `run()`. Demote back on every open so we stay hidden.
                app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }
        }
    );

    /// The application that asked us to open the URL currently being handled,
    /// read from the sender PID of the Apple Event AppKit is dispatching into
    /// `application:openURLs:`. Returns `None` when there is no such event
    /// (direct/terminal invocation) or no sender is attached (Spotlight, the
    /// `open` tool, some sandboxed callers).
    fn current_opener() -> Option<Opener> {
        // fourCC `keySenderPIDAttr` ('spid'): the sender's pid on the event.
        const KEY_SENDER_PID_ATTR: u32 = u32::from_be_bytes(*b"spid");

        let event = NSAppleEventManager::sharedAppleEventManager().currentAppleEvent()?;
        let pid_desc: Option<Retained<NSAppleEventDescriptor>> =
            unsafe { msg_send![&*event, attributeDescriptorForKeyword: KEY_SENDER_PID_ATTR] };
        let pid = pid_desc?.int32Value();
        if pid <= 0 {
            return None;
        }

        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        let opener = Opener {
            bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
            name: app.localizedName().map(|s| s.to_string()),
        };
        // Nothing to match on if the process exposed neither identifier.
        (opener.bundle_id.is_some() || opener.name.is_some()).then_some(opener)
    }

    impl Delegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            unsafe { msg_send![Self::alloc(mtm), init] }
        }
    }

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        // The delegate must outlive `run()` (NSApplication holds it weakly).
        let delegate = Delegate::new(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        app.run();
    }
}
