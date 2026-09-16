//! Menu-bar extra + top-of-screen creature. macOS only.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSBackingStoreType, NSColor, NSCompositingOperation,
    NSControlStateValueOff, NSControlStateValueOn, NSEvent, NSFont, NSImage, NSMenu, NSMenuItem,
    NSPanel, NSScreen, NSSecureTextField, NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
    NSStatusWindowLevel, NSTextField, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSData, NSNotification, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSSize, NSString, NSTimer,
};

use crate::client::{Client, ClientError};
use crate::discover;
use crate::display;
use crate::home;
use crate::mood::{self, Mood, MoodInput};
use crate::ollama::{self, Ollama, OllamaError};
use crate::origin::LoopbackOrigin;
use crate::validate;

const STRIP_H: f64 = 200.0;
const CREATURE_H: f64 = 88.0;
const BUBBLE_W: f64 = 320.0;
const BUBBLE_H: f64 = 56.0;
const INPUT_W: f64 = 280.0;
const INPUT_H: f64 = 24.0;
const CREATURE_Y: f64 = 8.0;
const INPUT_Y: f64 = 12.0;
const SPEED: f64 = 1.6;
const PNG: &[u8] = include_bytes!("../assets/creature.png");
const BACKEND_HARNESS: u8 = 0;
const BACKEND_OLLAMA: u8 = 1;
const DEFAULT_BACKEND: u8 = BACKEND_OLLAMA;

struct Shared {
    mood: Mutex<Mood>,
    last_reply: Mutex<String>,
    pending: Mutex<Option<String>>,
    pending_login: Mutex<Option<(String, String)>>,
    in_flight: AtomicBool,
    talking: AtomicBool,
    click: AtomicBool,
    backend: AtomicU8,
    port: AtomicU16,
    /// Whether the current harness `port` was last confirmed reachable over
    /// HTTPS (fresh homes) rather than plain HTTP (legacy `tls.enabled:
    /// false` homes). Read by both the status and chat worker threads so
    /// they agree on which `Client` constructor to use.
    scheme_https: AtomicBool,
}

struct Walk {
    x: f64,
    dir: f64,
    t: f64,
    strip_on: bool,
    bubble_on: bool,
    last_mood: Mood,
}

struct OverlayLayout {
    panel: NSRect,
    creature: NSPoint,
    bubble: NSPoint,
    input: NSPoint,
}

impl OverlayLayout {
    fn content_width(bubble_on: bool) -> f64 {
        if bubble_on {
            BUBBLE_W.max(cw_offset() + INPUT_W)
        } else {
            creature_width()
        }
    }

    fn new(screen: NSRect, x: f64, bob: f64, bubble_on: bool) -> Self {
        let creature_y = CREATURE_Y + bob;
        let bubble_y = CREATURE_Y + CREATURE_H + 4.0 + bob;
        let bottom = if bubble_on {
            creature_y.min(INPUT_Y)
        } else {
            creature_y
        };
        let top = if bubble_on {
            bubble_y + BUBBLE_H
        } else {
            creature_y + CREATURE_H
        };
        let strip_y = screen.origin.y + screen.size.height - STRIP_H;
        Self {
            panel: NSRect::new(
                NSPoint::new(screen.origin.x + x, strip_y + bottom),
                NSSize::new(Self::content_width(bubble_on), top - bottom),
            ),
            creature: NSPoint::new(0.0, creature_y - bottom),
            bubble: NSPoint::new(0.0, bubble_y - bottom),
            input: NSPoint::new(cw_offset(), INPUT_Y - bottom),
        }
    }
}

struct DelegateIvars {
    status_item: RefCell<Option<Retained<NSStatusItem>>>,
    panel: RefCell<Option<Retained<KeyPanel>>>,
    overlay: RefCell<Option<Retained<OverlayView>>>,
    creature: RefCell<Option<Retained<CreatureView>>>,
    bubble: RefCell<Option<Retained<NSTextField>>>,
    input: RefCell<Option<Retained<NSTextField>>>,
    harness_item: RefCell<Option<Retained<NSMenuItem>>>,
    ollama_item: RefCell<Option<Retained<NSMenuItem>>>,
    walk: RefCell<Walk>,
    shared: Arc<Shared>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _n: &NSNotification) {
            self.setup();
        }
        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _n: &NSNotification) {}
    }

    impl Delegate {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: Option<&NSTimer>) {
            self.on_tick();
        }

        #[unsafe(method(talk:))]
        fn talk_action(&self, _sender: Option<&AnyObject>) {
            self.open_talk();
        }

        #[unsafe(method(useHarness:))]
        fn use_harness_action(&self, _sender: Option<&AnyObject>) {
            self.set_backend(BACKEND_HARNESS);
        }

        #[unsafe(method(useOllama:))]
        fn use_ollama_action(&self, _sender: Option<&AnyObject>) {
            self.set_backend(BACKEND_OLLAMA);
        }

        #[unsafe(method(harnessLogin:))]
        fn harness_login_action(&self, _sender: Option<&AnyObject>) {
            self.prompt_harness_login();
        }

        #[unsafe(method(sendChat:))]
        fn send_chat_action(&self, _sender: Option<&AnyObject>) {
            self.send_chat();
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            NSApplication::sharedApplication(mtm).terminate(None);
        }
    }
);

struct CreatureIvars {
    image: Retained<NSImage>,
    shared: Arc<Shared>,
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = CreatureIvars]
    struct CreatureView;

    unsafe impl NSObjectProtocol for CreatureView {}

    impl CreatureView {
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let image = &self.ivars().image;
            let bounds = self.bounds();
            let mood = *self.ivars().shared.mood.lock().unwrap_or_else(|p| p.into_inner());
            let fraction = match mood {
                Mood::Asleep => 0.45,
                Mood::Sick => 0.85,
                _ => 1.0,
            };
            image.drawInRect_fromRect_operation_fraction(
                bounds,
                NSRect::ZERO,
                NSCompositingOperation::SourceOver,
                fraction,
            );
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: Option<&NSEvent>) {
            self.ivars().shared.click.store(true, Ordering::SeqCst);
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

struct OverlayIvars;

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = OverlayIvars]
    struct OverlayView;

    unsafe impl NSObjectProtocol for OverlayView {}

    impl OverlayView {
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            false
        }

        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
            let Some(view) = hit else {
                return std::ptr::null_mut();
            };
            let self_ptr = std::ptr::from_ref(self) as *const NSView;
            let hit_ptr = Retained::as_ptr(&view);
            if std::ptr::eq(self_ptr, hit_ptr) {
                std::ptr::null_mut()
            } else {
                Retained::autorelease_return(view)
            }
        }
    }
);

struct KeyPanelIvars;

define_class!(
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    #[ivars = KeyPanelIvars]
    struct KeyPanel;

    unsafe impl NSObjectProtocol for KeyPanel {}

    impl KeyPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            true
        }
    }
);

impl CreatureView {
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        image: Retained<NSImage>,
        shared: Arc<Shared>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(CreatureIvars { image, shared });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

impl OverlayView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(OverlayIvars);
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

impl KeyPanel {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(KeyPanelIvars);
        unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        }
    }
}

impl Delegate {
    fn new(mtm: MainThreadMarker, shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars {
            status_item: RefCell::new(None),
            panel: RefCell::new(None),
            overlay: RefCell::new(None),
            creature: RefCell::new(None),
            bubble: RefCell::new(None),
            input: RefCell::new(None),
            harness_item: RefCell::new(None),
            ollama_item: RefCell::new(None),
            walk: RefCell::new(Walk {
                x: 24.0,
                dir: 1.0,
                t: 0.0,
                strip_on: true,
                bubble_on: false,
                last_mood: Mood::Asleep,
            }),
            shared,
        });
        unsafe { msg_send![super(this), init] }
    }

    fn load_image() -> Retained<NSImage> {
        let data = NSData::with_bytes(PNG);
        NSImage::initWithData(NSImage::alloc(), &data).expect("creature png")
    }

    fn menu_item(mtm: MainThreadMarker, title: &NSString, action: Sel) -> Retained<NSMenuItem> {
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                title,
                Some(action),
                ns_string!(""),
            )
        }
    }

    fn setup(&self) {
        let mtm = self.mtm();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let image = Self::load_image();
        image.setSize(NSSize::new(18.0, 18.0));

        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(NSSquareStatusItemLength);
        if let Some(button) = item.button(mtm) {
            button.setImage(Some(&image));
        }

        let menu = NSMenu::new(mtm);
        let talk = Self::menu_item(mtm, ns_string!("Talk"), sel!(talk:));
        let harness = Self::menu_item(
            mtm,
            ns_string!("Harness (127.0.0.1:8790)"),
            sel!(useHarness:),
        );
        let ollama = Self::menu_item(
            mtm,
            ns_string!("Direct Ollama (qwen3.8:27b-mlx)"),
            sel!(useOllama:),
        );
        let login = Self::menu_item(mtm, ns_string!("Harness Login…"), sel!(harnessLogin:));
        let quit = Self::menu_item(mtm, ns_string!("Quit CG-Agent-MacOS-Avatar"), sel!(quit:));
        unsafe {
            talk.setTarget(Some(self.as_ref()));
            harness.setTarget(Some(self.as_ref()));
            ollama.setTarget(Some(self.as_ref()));
            login.setTarget(Some(self.as_ref()));
            quit.setTarget(Some(self.as_ref()));
        }
        harness.setState(NSControlStateValueOff);
        ollama.setState(NSControlStateValueOn);
        menu.addItem(&talk);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&harness);
        menu.addItem(&ollama);
        menu.addItem(&login);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&quit);
        item.setMenu(Some(&menu));
        *self.ivars().harness_item.borrow_mut() = Some(harness);
        *self.ivars().ollama_item.borrow_mut() = Some(ollama);

        let screen = NSScreen::mainScreen(mtm).expect("screen");
        let sf = screen.frame();
        let layout = OverlayLayout::new(sf, 24.0, 0.0, false);
        let panel = KeyPanel::new(mtm, layout.panel);
        unsafe {
            panel.setReleasedWhenClosed(false);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(false);
            panel.setLevel(NSStatusWindowLevel);
            panel.setFloatingPanel(true);
            panel.setBecomesKeyOnlyIfNeeded(false);
            panel.setHidesOnDeactivate(false);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
            panel.setIgnoresMouseEvents(false);
        }

        let overlay = OverlayView::new(mtm, NSRect::new(NSPoint::ZERO, layout.panel.size));
        overlay.setWantsLayer(true);
        panel.setContentView(Some(&overlay));

        let cw = creature_width();
        let creature_img = Self::load_image();
        creature_img.setSize(NSSize::new(cw, CREATURE_H));
        let creature = CreatureView::new(
            mtm,
            NSRect::new(layout.creature, NSSize::new(cw, CREATURE_H)),
            creature_img,
            Arc::clone(&self.ivars().shared),
        );
        overlay.addSubview(&creature);

        let bubble = {
            let b = NSTextField::labelWithString(ns_string!(""), mtm);
            b.setFrame(NSRect::new(layout.bubble, NSSize::new(BUBBLE_W, BUBBLE_H)));
            b.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            b.setTextColor(Some(&NSColor::labelColor()));
            b.setDrawsBackground(true);
            b.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
                1.0, 1.0, 1.0, 0.94,
            )));
            b.setHidden(true);
            overlay.addSubview(&b);
            b
        };
        let input = unsafe {
            let f = NSTextField::initWithFrame(
                NSTextField::alloc(mtm),
                NSRect::new(layout.input, NSSize::new(INPUT_W, INPUT_H)),
            );
            f.setEditable(true);
            f.setSelectable(true);
            f.setEnabled(true);
            f.setBezeled(true);
            f.setDrawsBackground(true);
            f.setPlaceholderString(Some(ns_string!("Type here, then Return")));
            f.setTarget(Some(self.as_ref()));
            f.setAction(Some(sel!(sendChat:)));
            f.setHidden(true);
            overlay.addSubview(&f);
            f
        };

        *self.ivars().status_item.borrow_mut() = Some(item);
        *self.ivars().panel.borrow_mut() = Some(panel.clone());
        *self.ivars().overlay.borrow_mut() = Some(overlay);
        *self.ivars().creature.borrow_mut() = Some(creature);
        *self.ivars().bubble.borrow_mut() = Some(bubble);
        *self.ivars().input.borrow_mut() = Some(input);

        panel.orderFront(None);

        unsafe {
            let _ = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                1.0 / 30.0,
                self.as_ref(),
                sel!(tick:),
                None,
                true,
            );
        }
        let _ = app;
    }

    fn set_backend(&self, backend: u8) {
        self.ivars().shared.backend.store(backend, Ordering::SeqCst);
        let harness_on = backend == BACKEND_HARNESS;
        if let Some(h) = self.ivars().harness_item.borrow().as_ref() {
            h.setState(if harness_on {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if let Some(o) = self.ivars().ollama_item.borrow().as_ref() {
            o.setState(if harness_on {
                NSControlStateValueOff
            } else {
                NSControlStateValueOn
            });
        }
        let note = if harness_on {
            "harness mode — needs serve :8790 for replies"
        } else {
            "direct ollama — qwen3.8:27b-mlx on :11434"
        };
        *self.ivars().shared.last_reply.lock().unwrap() = note.into();
        self.open_talk();
    }

    /// Two sequential modal prompts (username, then password) rather than
    /// one combined accessory view — smaller surface to get wrong, and this
    /// is a rarely-used flow. Credentials are handed to the chat worker
    /// thread rather than sent from here: `Client::login` is a blocking
    /// network call and must not run on the main/UI thread, and it must
    /// reuse the exact same `Client` (and its cookie jar) the chat thread
    /// already owns so the resulting session actually applies to later
    /// chats.
    fn prompt_harness_login(&self) {
        let mtm = self.mtm();
        let Some(username) = Self::prompt_plain_text(mtm, "Harness Login", "Username") else {
            return;
        };
        let Some(password) = Self::prompt_secure_text(mtm, "Harness Login", "Password") else {
            return;
        };
        if username.trim().is_empty() || password.is_empty() {
            return;
        }
        *self.ivars().shared.pending_login.lock().unwrap() = Some((username, password));
        *self.ivars().shared.last_reply.lock().unwrap() = "logging in…".into();
        self.open_talk();
    }

    fn prompt_plain_text(mtm: MainThreadMarker, message: &str, info: &str) -> Option<String> {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        alert.setInformativeText(&NSString::from_str(info));
        alert.addButtonWithTitle(ns_string!("OK"));
        alert.addButtonWithTitle(ns_string!("Cancel"));
        let field = NSTextField::initWithFrame(
            NSTextField::alloc(mtm),
            NSRect::new(NSPoint::ZERO, NSSize::new(240.0, 24.0)),
        );
        alert.setAccessoryView(Some(&field));
        if alert.runModal() != NSAlertFirstButtonReturn {
            return None;
        }
        Some(field.stringValue().to_string())
    }

    fn prompt_secure_text(mtm: MainThreadMarker, message: &str, info: &str) -> Option<String> {
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(message));
        alert.setInformativeText(&NSString::from_str(info));
        alert.addButtonWithTitle(ns_string!("OK"));
        alert.addButtonWithTitle(ns_string!("Cancel"));
        let field = NSSecureTextField::initWithFrame(
            NSSecureTextField::alloc(mtm),
            NSRect::new(NSPoint::ZERO, NSSize::new(240.0, 24.0)),
        );
        alert.setAccessoryView(Some(&field));
        if alert.runModal() != NSAlertFirstButtonReturn {
            return None;
        }
        Some(field.stringValue().to_string())
    }

    fn open_talk(&self) {
        {
            let mut walk = self.ivars().walk.borrow_mut();
            walk.strip_on = true;
            walk.bubble_on = true;
        }
        self.apply_layout(OverlayLayout::new(
            NSScreen::mainScreen(self.mtm()).expect("screen").frame(),
            self.ivars().walk.borrow().x,
            0.0,
            true,
        ));
        if let Some(panel) = self.ivars().panel.borrow().as_ref() {
            panel.orderFront(None);
            panel.makeKeyAndOrderFront(None);
        }
        if let Some(b) = self.ivars().bubble.borrow().as_ref() {
            b.setHidden(false);
        }
        if let Some(i) = self.ivars().input.borrow().as_ref() {
            i.setHidden(false);
            let mtm = self.mtm();
            NSApplication::sharedApplication(mtm).activate();
            if let Some(panel) = self.ivars().panel.borrow().as_ref() {
                let _ = panel.makeFirstResponder(Some(i));
            }
        }
    }

    fn send_chat(&self) {
        let input = self.ivars().input.borrow();
        let Some(field) = input.as_ref() else {
            return;
        };
        let value = field.stringValue().to_string();
        let Ok(trimmed) = validate::message(&value) else {
            return;
        };
        *self.ivars().shared.pending.lock().unwrap() = Some(trimmed.to_string());
        field.setStringValue(ns_string!(""));
        self.open_talk();
    }

    fn on_tick(&self) {
        if self.ivars().shared.click.swap(false, Ordering::SeqCst) {
            self.open_talk();
        }

        let mood = *self
            .ivars()
            .shared
            .mood
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let screen = NSScreen::mainScreen(self.mtm()).expect("screen").frame();
        let mut walk = self.ivars().walk.borrow_mut();
        if !walk.strip_on {
            drop(walk);
            return;
        }
        walk.t += 1.0;
        let max_x =
            (screen.size.width - OverlayLayout::content_width(walk.bubble_on) - 16.0).max(16.0);
        match mood {
            Mood::Thinking | Mood::Asleep => {}
            _ => {
                walk.x += walk.dir * SPEED;
                if walk.x > max_x {
                    walk.x = max_x;
                    walk.dir = -1.0;
                } else if walk.x < 16.0 {
                    walk.x = 16.0;
                    walk.dir = 1.0;
                }
            }
        }
        let bob = match mood {
            Mood::Thinking => (walk.t * 0.4).sin() * 8.0,
            Mood::Talking => 4.0,
            _ => ((walk.t * 0.15).sin() * 2.0).abs(),
        };
        let x = walk.x;
        let bubble_on = walk.bubble_on;
        let mood_changed = walk.last_mood != mood;
        walk.last_mood = mood;
        drop(walk);

        self.apply_layout(OverlayLayout::new(screen, x, bob, bubble_on));
        if mood_changed {
            if let Some(c) = self.ivars().creature.borrow().as_ref() {
                c.setNeedsDisplay(true);
            }
        }

        if bubble_on {
            if let Some(b) = self.ivars().bubble.borrow().as_ref() {
                let reply = self.ivars().shared.last_reply.lock().unwrap().clone();
                let text = if self.ivars().shared.in_flight.load(Ordering::Relaxed) {
                    "…thinking".to_string()
                } else if reply.is_empty() {
                    "type below, Return to send".to_string()
                } else {
                    display::bubble_text(&reply)
                };
                b.setStringValue(&NSString::from_str(&text));
            }
        }
    }

    fn apply_layout(&self, layout: OverlayLayout) {
        if let Some(panel) = self.ivars().panel.borrow().as_ref() {
            panel.setFrame_display(layout.panel, false);
        }
        if let Some(overlay) = self.ivars().overlay.borrow().as_ref() {
            overlay.setFrame(NSRect::new(NSPoint::ZERO, layout.panel.size));
        }
        if let Some(creature) = self.ivars().creature.borrow().as_ref() {
            creature.setFrameOrigin(layout.creature);
        }
        if let Some(bubble) = self.ivars().bubble.borrow().as_ref() {
            bubble.setFrameOrigin(layout.bubble);
        }
        if let Some(input) = self.ivars().input.borrow().as_ref() {
            input.setFrameOrigin(layout.input);
        }
    }
}

fn creature_width() -> f64 {
    CREATURE_H * (589.0 / 778.0)
}

fn cw_offset() -> f64 {
    creature_width() + 8.0
}

/// A `LoopbackOrigin` + `Client` for the last-resolved harness scheme.
/// HTTPS requires the harness's own pinned leaf certificate
/// (`home::read_pinned_cert`) — with no cert available, an HTTPS-resolved
/// port simply cannot be connected to yet (that itself is a meaningful,
/// non-"asleep" state, surfaced by the caller checking for `None`).
fn build_harness_client(port: u16, https: bool, home_dir: &std::path::Path) -> Option<Client> {
    if https {
        let cert = home::read_pinned_cert(home_dir)?;
        Client::new_https(LoopbackOrigin::from_port_https(port), &cert).ok()
    } else {
        Client::from_port(port).ok()
    }
}

fn spawn_workers(shared: Arc<Shared>) {
    let home_dir = home::default_home();
    let preferred = home::port_from_home(&home_dir);
    shared.port.store(preferred, Ordering::SeqCst);
    let status_ollama = Ollama::new().ok();
    std::thread::Builder::new()
        .name("cg-agent-status".into())
        .spawn({
            let shared = Arc::clone(&shared);
            let home_dir = home_dir.clone();
            move || loop {
                let backend = shared.backend.load(Ordering::Relaxed);
                let input = if backend == BACKEND_OLLAMA {
                    match status_ollama
                        .as_ref()
                        .ok_or(OllamaError::Unreachable)
                        .and_then(|o| o.tags_ok())
                    {
                        Ok(true) => MoodInput {
                            reachable: true,
                            http_ok: true,
                            api_key_optional: true,
                            model: ollama::MODEL.into(),
                            provider: "ollama".into(),
                            chat_in_flight: shared.in_flight.load(Ordering::Relaxed),
                            talking_until: shared.talking.load(Ordering::Relaxed),
                        },
                        Ok(false) => MoodInput {
                            reachable: true,
                            http_ok: false,
                            api_key_optional: true,
                            model: String::new(),
                            provider: "ollama".into(),
                            chat_in_flight: false,
                            talking_until: false,
                        },
                        Err(OllamaError::Unreachable) => MoodInput {
                            reachable: false,
                            http_ok: false,
                            api_key_optional: true,
                            model: String::new(),
                            provider: String::new(),
                            chat_in_flight: false,
                            talking_until: false,
                        },
                        Err(_) => MoodInput {
                            reachable: true,
                            http_ok: false,
                            api_key_optional: true,
                            model: String::new(),
                            provider: "ollama".into(),
                            chat_in_flight: false,
                            talking_until: false,
                        },
                    }
                } else {
                    {
                        let pinned_cert = home::read_pinned_cert(&home_dir);
                        let current_port = shared.port.load(Ordering::Relaxed);
                        let current_https = shared.scheme_https.load(Ordering::Relaxed);
                        // Re-probe the currently-cached port/scheme first so
                        // a healthy connection doesn't pay for a full
                        // rediscovery sweep every 2s; only fall back to
                        // discover::resolve_reachable (https-first, then
                        // legacy http, across the lsof candidate list) when
                        // that fails.
                        let still_ok = if current_https {
                            pinned_cert.as_deref().is_some_and(|cert| {
                                discover::probe_harness_https(current_port, cert)
                            })
                        } else {
                            discover::probe_harness(current_port)
                        };
                        let reachable = if still_ok {
                            Some(if current_https {
                                discover::Reachable::Https(current_port)
                            } else {
                                discover::Reachable::Http(current_port)
                            })
                        } else {
                            discover::resolve_reachable(preferred, pinned_cert.as_deref())
                        };
                        match reachable {
                            None => MoodInput {
                                reachable: false,
                                http_ok: false,
                                api_key_optional: true,
                                model: String::new(),
                                provider: String::new(),
                                chat_in_flight: false,
                                talking_until: false,
                            },
                            Some(r) => {
                                let (port, https) = match r {
                                    discover::Reachable::Http(p) => (p, false),
                                    discover::Reachable::Https(p) => (p, true),
                                };
                                shared.port.store(port, Ordering::SeqCst);
                                shared.scheme_https.store(https, Ordering::SeqCst);
                                match build_harness_client(port, https, &home_dir)
                                    .ok_or(ClientError::Unreachable)
                                    .and_then(|c| c.status())
                                {
                                    Ok(s) if !s.model.is_empty() => MoodInput {
                                        reachable: true,
                                        http_ok: true,
                                        api_key_optional: s.api_key_optional,
                                        model: s.model,
                                        provider: s.provider,
                                        chat_in_flight: shared.in_flight.load(Ordering::Relaxed),
                                        talking_until: shared.talking.load(Ordering::Relaxed),
                                    },
                                    // Includes the thin, login-required shape
                                    // (found the harness, no model yet since
                                    // this client hasn't logged in) — Sick is
                                    // the closest existing mood, matching
                                    // pre-HTTPS behavior for this same case.
                                    Ok(_) => MoodInput {
                                        reachable: true,
                                        http_ok: false,
                                        api_key_optional: true,
                                        model: String::new(),
                                        provider: String::new(),
                                        chat_in_flight: false,
                                        talking_until: false,
                                    },
                                    Err(ClientError::Unreachable) => MoodInput {
                                        reachable: false,
                                        http_ok: false,
                                        api_key_optional: true,
                                        model: String::new(),
                                        provider: String::new(),
                                        chat_in_flight: false,
                                        talking_until: false,
                                    },
                                    Err(_) => MoodInput {
                                        reachable: true,
                                        http_ok: false,
                                        api_key_optional: true,
                                        model: String::new(),
                                        provider: String::new(),
                                        chat_in_flight: false,
                                        talking_until: false,
                                    },
                                }
                            }
                        }
                    }
                };
                *shared.mood.lock().unwrap() = mood::mood(input);
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("status thread");

    std::thread::Builder::new()
        .name("cg-agent-chat".into())
        .spawn({
            let shared = Arc::clone(&shared);
            let home_dir = home_dir.clone();
            move || {
                let mut last_port = 0u16;
                let mut last_https = false;
                let mut harness = None::<Client>;
                let ollama = Ollama::new().ok();
                let mut session = None::<String>;
                loop {
                    let login = { shared.pending_login.lock().unwrap().take() };
                    if let Some((username, password)) = login {
                        let port = shared.port.load(Ordering::SeqCst);
                        let https = shared.scheme_https.load(Ordering::SeqCst);
                        if port != last_port || https != last_https || harness.is_none() {
                            harness = build_harness_client(port, https, &home_dir);
                            session = None;
                            last_port = port;
                            last_https = https;
                        }
                        match harness.as_mut() {
                            Some(client) => match client.login(&username, &password) {
                                Ok(info) if info.must_change_password => {
                                    *shared.last_reply.lock().unwrap() =
                                        "logged in — the bootstrap password must be changed in the harness console before chatting".into();
                                }
                                Ok(info) => {
                                    session = None;
                                    *shared.last_reply.lock().unwrap() =
                                        format!("logged in as {}", info.username);
                                }
                                Err(e) => {
                                    *shared.last_reply.lock().unwrap() = format!("login failed: {e}");
                                }
                            },
                            None => {
                                *shared.last_reply.lock().unwrap() = "harness asleep".into();
                            }
                        }
                    }

                    let msg = { shared.pending.lock().unwrap().take() };
                    if let Some(message) = msg {
                        shared.in_flight.store(true, Ordering::SeqCst);
                        shared.talking.store(false, Ordering::SeqCst);
                        let backend = shared.backend.load(Ordering::SeqCst);
                        if backend == BACKEND_OLLAMA {
                            let result = ollama
                                .as_ref()
                                .ok_or(OllamaError::Unreachable)
                                .and_then(|o| o.chat(&message));
                            match result {
                                Ok(reply) => {
                                    *shared.last_reply.lock().unwrap() = reply;
                                    shared.talking.store(true, Ordering::SeqCst);
                                }
                                Err(OllamaError::Unreachable) => {
                                    *shared.last_reply.lock().unwrap() = "ollama asleep".into();
                                }
                                Err(OllamaError::ModelMissing) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "pull qwen3.8:27b-mlx".into();
                                }
                                Err(e) => {
                                    *shared.last_reply.lock().unwrap() = format!("ollama: {e}");
                                }
                            }
                        } else {
                            let result = (|| {
                                let port = shared.port.load(Ordering::SeqCst);
                                let https = shared.scheme_https.load(Ordering::SeqCst);
                                if port != last_port || https != last_https || harness.is_none() {
                                    harness = build_harness_client(port, https, &home_dir);
                                    session = None;
                                    last_port = port;
                                    last_https = https;
                                }
                                let client = harness.as_mut().ok_or(ClientError::Unreachable)?;
                                if session.is_none() {
                                    session = Some(client.ensure_session()?);
                                }
                                client.chat(session.as_deref().unwrap(), &message)
                            })();
                            match result {
                                Ok(reply) => {
                                    let mut text = reply.reply;
                                    if !reply.web_tools.is_empty() {
                                        text = display::with_web_tools_note(&text, reply.web_tools.len());
                                    }
                                    *shared.last_reply.lock().unwrap() = text;
                                    shared.talking.store(true, Ordering::SeqCst);
                                }
                                Err(ClientError::ChatBusy) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "busy — wait a beat".into();
                                }
                                Err(ClientError::LoginRequired) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "login required — use Harness Login… in the menu".into();
                                }
                                Err(ClientError::PasswordChangeRequired) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "bootstrap password must be changed — use the harness console".into();
                                }
                                Err(ClientError::CertMismatch) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "harness certificate changed — verify it, then re-check trust".into();
                                }
                                Err(ClientError::KeyRequired) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "key required — use the console".into();
                                }
                                Err(ClientError::Unreachable) => {
                                    *shared.last_reply.lock().unwrap() = "harness asleep".into();
                                }
                                Err(e) => {
                                    *shared.last_reply.lock().unwrap() = format!("nope: {e}");
                                }
                            }
                        }
                        shared.in_flight.store(false, Ordering::SeqCst);
                        std::thread::spawn({
                            let shared = Arc::clone(&shared);
                            move || {
                                std::thread::sleep(Duration::from_secs(8));
                                shared.talking.store(false, Ordering::SeqCst);
                            }
                        });
                    } else {
                        std::thread::sleep(Duration::from_millis(80));
                    }
                }
            }
        })
        .expect("chat thread");
}

pub fn run() {
    let shared = Arc::new(Shared {
        mood: Mutex::new(Mood::Asleep),
        last_reply: Mutex::new(String::new()),
        pending: Mutex::new(None),
        pending_login: Mutex::new(None),
        in_flight: AtomicBool::new(false),
        talking: AtomicBool::new(false),
        click: AtomicBool::new(false),
        backend: AtomicU8::new(DEFAULT_BACKEND),
        port: AtomicU16::new(home::DEFAULT_PORT),
        scheme_https: AtomicBool::new(false),
    });
    spawn_workers(Arc::clone(&shared));

    let mtm = MainThreadMarker::new().expect("GUI on main thread");
    let app = NSApplication::sharedApplication(mtm);
    let delegate = Delegate::new(mtm, shared);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_ollama_is_the_launch_default() {
        assert_eq!(DEFAULT_BACKEND, BACKEND_OLLAMA);
    }

    #[test]
    fn panel_is_only_as_large_as_the_interactive_views() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let idle = OverlayLayout::new(screen, 100.0, 0.0, false);
        assert_eq!(idle.panel.size.width, creature_width());
        assert_eq!(idle.panel.size.height, CREATURE_H);

        let talk = OverlayLayout::new(screen, 100.0, 8.0, true);
        assert_eq!(talk.panel.size.width, cw_offset() + INPUT_W);
        assert_eq!(talk.panel.size.height, BUBBLE_H + CREATURE_H + 8.0);
        assert_eq!(talk.creature.x, 0.0);
        assert!(talk.input.y >= 0.0);
        assert!(talk.bubble.y + BUBBLE_H <= talk.panel.size.height);
    }
}
