//! Menu-bar extra + top-of-screen creature. macOS only.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSBackingStoreType, NSButton, NSColor, NSCompositingOperation,
    NSControlStateValueOff, NSControlStateValueOn, NSEvent, NSFont, NSForegroundColorAttributeName,
    NSImage, NSMenu, NSMenuItem, NSPanel, NSScreen, NSScrollView, NSSecureTextField,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem, NSStatusWindowLevel, NSTextField,
    NSTextView, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSAttributedString, NSData, NSDictionary, NSNotification,
    NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer,
};

use crate::client::{Client, ClientError, SessionJar, Status};
use crate::discover;
use crate::display;
use crate::harness_setup::{Guide, Phase};
use crate::home;
use crate::mood::{self, Health, Mood};
use crate::ollama::{self, Ollama, OllamaError};
use crate::origin::LoopbackOrigin;
use crate::theme;
use crate::validate;

const PNG: &[u8] = include_bytes!("../assets/creature.png");
const BACKEND_HARNESS: u8 = 0;
const BACKEND_OLLAMA: u8 = 1;
const DEFAULT_BACKEND: u8 = BACKEND_OLLAMA;
/// How often the selected backend is checked over the network. Harness counts
/// these against its per-IP API budget (60/min by default), shared with its
/// own console, so this stays well under it.
const POLL_EVERY: Duration = Duration::from_secs(5);
/// How often the mood is re-derived from the last check plus chat state.
const MOOD_EVERY: Duration = Duration::from_millis(250);

struct Shared {
    mood: Mutex<Mood>,
    last_reply: Mutex<String>,
    pending: Mutex<Option<(u8, u64, String)>>,
    pending_login: Mutex<Option<(u64, String, String)>>,
    pending_password_change: Mutex<Option<(u64, String, String)>>,
    in_flight: AtomicBool,
    talking: AtomicBool,
    click: AtomicBool,
    backend: AtomicU8,
    guide: Mutex<Guide>,
    /// One in-memory cookie jar for every harness client, so the status
    /// check sees the chat worker's login instead of the pre-login view.
    jar: Arc<SessionJar>,
    /// Set after login or a password change to re-check status right away.
    repoll: AtomicBool,
}

struct Walk {
    x: f64,
    dir: f64,
    t: f64,
    strip_on: bool,
    bubble_on: bool,
    reply_expanded: bool,
    last_mood: Mood,
}

/// Text and height are independent of the creature's animation. Retain them
/// until the reply or thinking state changes; replacing NSTextView's contents
/// on each tick also discards a user's selection.
struct ReplyPresentation {
    raw: String,
    in_flight: bool,
    preview: String,
    full: String,
    heights: (f64, f64),
}

impl ReplyPresentation {
    fn new(metrics: &theme::Metrics) -> Self {
        let full = reply_text("", false);
        Self {
            raw: String::new(),
            in_flight: false,
            preview: preview_text("", false),
            heights: expanded_reply_heights(&full, metrics),
            full,
        }
    }

    fn update(&mut self, reply: &str, in_flight: bool, metrics: &theme::Metrics) -> bool {
        if self.raw == reply && self.in_flight == in_flight {
            return false;
        }
        self.raw.clear();
        self.raw.push_str(reply);
        self.in_flight = in_flight;
        self.preview = preview_text(reply, in_flight);
        self.full = reply_text(reply, in_flight);
        self.heights = expanded_reply_heights(&self.full, metrics);
        true
    }
}

struct OverlayLayout {
    panel: NSRect,
    creature: NSPoint,
    bubble: NSPoint,
    scroll: NSPoint,
    button: NSPoint,
    input: NSPoint,
    reply_size: NSSize,
    text_size: NSSize,
}

impl OverlayLayout {
    fn content_width(bubble_on: bool, metrics: &theme::Metrics) -> f64 {
        let theme::Metrics {
            bubble_w: BUBBLE_W,
            input_w: INPUT_W,
            ..
        } = *metrics;
        if bubble_on {
            BUBBLE_W.max(cw_offset(metrics) + INPUT_W)
        } else {
            creature_width(metrics)
        }
    }

    // Rebuilt from scratch on every animation tick from flat, non-optional
    // layout inputs; a builder or config struct would add indirection with
    // no caller left to simplify for.
    #[allow(clippy::too_many_arguments)]
    fn new(
        screen: NSRect,
        x: f64,
        bob: f64,
        bubble_on: bool,
        reply_expanded: bool,
        reply_height: f64,
        text_height: f64,
        metrics: &theme::Metrics,
    ) -> Self {
        let theme::Metrics {
            creature_y: CREATURE_Y,
            creature_h: CREATURE_H,
            input_y: INPUT_Y,
            bubble_h: BUBBLE_H,
            bubble_w: BUBBLE_W,
            strip_h: STRIP_H,
            see_more_w: SEE_MORE_W,
            ..
        } = *metrics;
        let creature_y = CREATURE_Y + bob;
        let bubble_y = CREATURE_Y + CREATURE_H + 4.0 + bob;
        let bottom = if bubble_on {
            creature_y.min(INPUT_Y)
        } else {
            creature_y
        };
        let top = if bubble_on {
            bubble_y
                + if reply_expanded {
                    reply_height
                } else {
                    BUBBLE_H
                }
        } else {
            creature_y + CREATURE_H
        };
        let strip_y = screen.origin.y + screen.size.height - STRIP_H;
        Self {
            panel: NSRect::new(
                NSPoint::new(screen.origin.x + x, strip_y + bottom),
                NSSize::new(Self::content_width(bubble_on, metrics), top - bottom),
            ),
            creature: NSPoint::new(0.0, creature_y - bottom),
            bubble: NSPoint::new(0.0, bubble_y - bottom),
            scroll: NSPoint::new(0.0, bubble_y - bottom),
            button: NSPoint::new(BUBBLE_W - SEE_MORE_W - 6.0, bubble_y - bottom + 4.0),
            input: NSPoint::new(cw_offset(metrics), INPUT_Y - bottom),
            reply_size: NSSize::new(BUBBLE_W, reply_height),
            text_size: NSSize::new(BUBBLE_W, text_height),
        }
    }
}

struct DelegateIvars {
    status_item: RefCell<Option<Retained<NSStatusItem>>>,
    panel: RefCell<Option<Retained<KeyPanel>>>,
    overlay: RefCell<Option<Retained<OverlayView>>>,
    creature: RefCell<Option<Retained<CreatureView>>>,
    bubble: RefCell<Option<Retained<NSTextField>>>,
    reply_scroll: RefCell<Option<Retained<NSScrollView>>>,
    reply_text: RefCell<Option<Retained<NSTextView>>>,
    see_more: RefCell<Option<Retained<NSButton>>>,
    input: RefCell<Option<Retained<NSTextField>>>,
    harness_item: RefCell<Option<Retained<NSMenuItem>>>,
    ollama_item: RefCell<Option<Retained<NSMenuItem>>>,
    walk: RefCell<Walk>,
    reply_presentation: RefCell<ReplyPresentation>,
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
            if !crate::launch::launch_harness_app() {
                *self.ivars().shared.last_reply.lock().unwrap() =
                    "CG Agent Harness.app not found — install it, or run `cgagentharness serve`"
                        .into();
            }
        }

        #[unsafe(method(useOllama:))]
        fn use_ollama_action(&self, _sender: Option<&AnyObject>) {
            self.set_backend(BACKEND_OLLAMA);
        }

        #[unsafe(method(harnessLogin:))]
        fn harness_login_action(&self, _sender: Option<&AnyObject>) {
            self.prompt_harness_login();
        }

        #[unsafe(method(harnessPasswordReset:))]
        fn harness_password_reset_action(&self, _sender: Option<&AnyObject>) {
            self.prompt_harness_password_reset();
        }

        #[unsafe(method(sendChat:))]
        fn send_chat_action(&self, _sender: Option<&AnyObject>) {
            self.send_chat();
        }

        #[unsafe(method(toggleReply:))]
        fn toggle_reply_action(&self, _sender: Option<&AnyObject>) {
            self.toggle_reply();
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
            let motion = theme::active().motion;
            let fraction = match mood {
                Mood::Asleep => motion.asleep_opacity,
                Mood::Sick => motion.sick_opacity,
                _ => motion.awake_opacity,
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
            reply_scroll: RefCell::new(None),
            reply_text: RefCell::new(None),
            see_more: RefCell::new(None),
            input: RefCell::new(None),
            harness_item: RefCell::new(None),
            ollama_item: RefCell::new(None),
            walk: RefCell::new(Walk {
                x: 24.0,
                dir: 1.0,
                t: 0.0,
                strip_on: true,
                bubble_on: false,
                reply_expanded: false,
                last_mood: Mood::Asleep,
            }),
            reply_presentation: RefCell::new(ReplyPresentation::new(&theme::active().metrics)),
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
        let active = theme::active();
        let theme::Metrics {
            bubble_w: BUBBLE_W,
            bubble_h: BUBBLE_H,
            creature_h: CREATURE_H,
            see_more_w: SEE_MORE_W,
            see_more_h: SEE_MORE_H,
            input_w: INPUT_W,
            input_h: INPUT_H,
            ..
        } = active.metrics;
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
        let reset = Self::menu_item(
            mtm,
            ns_string!("Harness Password Reset…"),
            sel!(harnessPasswordReset:),
        );
        let quit = Self::menu_item(mtm, ns_string!("Quit CG-Agent-MacOS-Avatar"), sel!(quit:));
        unsafe {
            talk.setTarget(Some(self.as_ref()));
            harness.setTarget(Some(self.as_ref()));
            ollama.setTarget(Some(self.as_ref()));
            login.setTarget(Some(self.as_ref()));
            reset.setTarget(Some(self.as_ref()));
            quit.setTarget(Some(self.as_ref()));
        }
        harness.setState(NSControlStateValueOff);
        ollama.setState(NSControlStateValueOn);
        menu.addItem(&talk);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&harness);
        menu.addItem(&ollama);
        menu.addItem(&login);
        menu.addItem(&reset);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&quit);
        item.setMenu(Some(&menu));
        *self.ivars().harness_item.borrow_mut() = Some(harness);
        *self.ivars().ollama_item.borrow_mut() = Some(ollama);

        let screen = NSScreen::mainScreen(mtm).expect("screen");
        let sf = screen.frame();
        let layout = OverlayLayout::new(
            sf,
            24.0,
            0.0,
            false,
            false,
            BUBBLE_H,
            BUBBLE_H,
            &active.metrics,
        );
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

        let cw = creature_width(&active.metrics);
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
            let b = NSTextField::labelWithString(
                &NSString::from_str(&self.ivars().reply_presentation.borrow().preview),
                mtm,
            );
            b.setFrame(NSRect::new(layout.bubble, NSSize::new(BUBBLE_W, BUBBLE_H)));
            b.setFont(Some(&NSFont::systemFontOfSize(active.palette.font_size)));
            b.setTextColor(Some(&text_color(active)));
            b.setDrawsBackground(true);
            b.setBackgroundColor(Some(&ns_color(active.palette.bubble_background)));
            b.setHidden(true);
            overlay.addSubview(&b);
            b
        };
        let reply_text = {
            let text = NSTextView::initWithFrame(
                NSTextView::alloc(mtm),
                NSRect::new(NSPoint::ZERO, NSSize::new(BUBBLE_W, BUBBLE_H)),
            );
            text.setEditable(false);
            text.setSelectable(true);
            text.setRichText(false);
            text.setString(&NSString::from_str(
                &self.ivars().reply_presentation.borrow().full,
            ));
            text.setDrawsBackground(false);
            text.setFont(Some(&NSFont::systemFontOfSize(active.palette.font_size)));
            text.setTextColor(Some(&text_color(active)));
            text
        };
        let reply_scroll = {
            let scroll = NSScrollView::initWithFrame(
                NSScrollView::alloc(mtm),
                NSRect::new(layout.scroll, layout.reply_size),
            );
            scroll.setHasVerticalScroller(true);
            scroll.setHasHorizontalScroller(false);
            scroll.setAutohidesScrollers(true);
            scroll.setDrawsBackground(true);
            scroll.setBackgroundColor(&ns_color(active.palette.bubble_background));
            scroll.setDocumentView(Some(&reply_text));
            scroll.setHidden(true);
            overlay.addSubview(&scroll);
            scroll
        };
        let see_more = {
            let button = NSButton::initWithFrame(
                NSButton::alloc(mtm),
                NSRect::new(layout.button, NSSize::new(SEE_MORE_W, SEE_MORE_H)),
            );
            set_reply_button_title(&button, ns_string!("See More"));
            unsafe {
                button.setTarget(Some(self.as_ref()));
                button.setAction(Some(sel!(toggleReply:)));
            }
            button.setHidden(true);
            overlay.addSubview(&button);
            button
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
        *self.ivars().reply_scroll.borrow_mut() = Some(reply_scroll);
        *self.ivars().reply_text.borrow_mut() = Some(reply_text);
        *self.ivars().see_more.borrow_mut() = Some(see_more);
        *self.ivars().input.borrow_mut() = Some(input);

        panel.orderFront(None);

        unsafe {
            let _ = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                active.motion.tick_interval,
                self.as_ref(),
                sel!(tick:),
                None,
                true,
            );
        }
        let _ = app;
    }

    fn set_backend(&self, backend: u8) {
        let shared = &self.ivars().shared;
        let harness_on = backend == BACKEND_HARNESS;
        shared.guide.lock().unwrap().select(harness_on);
        shared.backend.store(backend, Ordering::SeqCst);
        shared.in_flight.store(false, Ordering::SeqCst);
        shared.talking.store(false, Ordering::SeqCst);
        shared.repoll.store(true, Ordering::SeqCst);
        self.ivars().walk.borrow_mut().reply_expanded = false;
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
        let note = shared.guide.lock().unwrap().phase().guidance();
        *self.ivars().shared.last_reply.lock().unwrap() = note.into();
        self.open_talk();
    }

    /// Login runs on the chat worker because it is a blocking network call.
    /// That worker also owns the client and cookie jar used for later chats.
    fn prompt_harness_login(&self) {
        let (generation, endpoint) = {
            let guide = self.ivars().shared.guide.lock().unwrap();
            let generation = guide.generation();
            (generation, guide.endpoint(generation))
        };
        if endpoint.is_none() {
            let guidance = self.ivars().shared.guide.lock().unwrap().phase().guidance();
            *self.ivars().shared.last_reply.lock().unwrap() = guidance.into();
            self.open_talk();
            return;
        }
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
        *self.ivars().shared.pending_login.lock().unwrap() = Some((generation, username, password));
        *self.ivars().shared.last_reply.lock().unwrap() = "logging in…".into();
        self.open_talk();
    }

    /// The harness permits an account flagged for bootstrap replacement to
    /// change only its own password. The request runs on the chat worker.
    fn prompt_harness_password_reset(&self) {
        let generation = {
            let guide = self.ivars().shared.guide.lock().unwrap();
            if !matches!(guide.phase(), Phase::Reset(_)) {
                *self.ivars().shared.last_reply.lock().unwrap() = guide.phase().guidance().into();
                self.open_talk();
                return;
            }
            guide.generation()
        };
        let mtm = self.mtm();
        let Some(current_password) =
            Self::prompt_secure_text(mtm, "Harness Password Reset", "Current password")
        else {
            return;
        };
        let Some(password) = Self::prompt_secure_text(
            mtm,
            "Harness Password Reset",
            "New password (at least 12 characters)",
        ) else {
            return;
        };
        if current_password.is_empty() || password.chars().count() < 12 {
            *self.ivars().shared.last_reply.lock().unwrap() =
                "new password needs at least 12 characters".into();
            self.open_talk();
            return;
        }
        *self.ivars().shared.pending_password_change.lock().unwrap() =
            Some((generation, current_password, password));
        *self.ivars().shared.last_reply.lock().unwrap() = "changing password…".into();
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
        let metrics = theme::active().metrics;
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
            false,
            metrics.bubble_h,
            metrics.bubble_h,
            &metrics,
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
        let backend = self.ivars().shared.backend.load(Ordering::SeqCst);
        let generation = if backend == BACKEND_HARNESS {
            let guide = self.ivars().shared.guide.lock().unwrap();
            let generation = guide.generation();
            if guide.ready_endpoint(generation).is_none() {
                *self.ivars().shared.last_reply.lock().unwrap() = guide.phase().guidance().into();
                self.open_talk();
                return;
            }
            generation
        } else {
            self.ivars().shared.guide.lock().unwrap().generation()
        };
        *self.ivars().shared.pending.lock().unwrap() =
            Some((backend, generation, trimmed.to_string()));
        self.ivars().walk.borrow_mut().reply_expanded = false;
        field.setStringValue(ns_string!(""));
        self.open_talk();
    }

    fn toggle_reply(&self) {
        let mut walk = self.ivars().walk.borrow_mut();
        if !walk.bubble_on {
            return;
        }
        walk.reply_expanded = !walk.reply_expanded;
        drop(walk);
        self.on_tick();
    }

    fn on_tick(&self) {
        if self.ivars().shared.click.swap(false, Ordering::SeqCst) {
            self.open_talk();
        }

        let active = theme::active();
        let motion = active.motion;
        let mood = *self
            .ivars()
            .shared
            .mood
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let screen = NSScreen::mainScreen(self.mtm()).expect("screen").frame();
        let in_flight = self.ivars().shared.in_flight.load(Ordering::Relaxed);
        let mut presentation = self.ivars().reply_presentation.borrow_mut();
        let text_changed = presentation.update(
            &self.ivars().shared.last_reply.lock().unwrap(),
            in_flight,
            &active.metrics,
        );
        let (reply_height, text_height) = presentation.heights;
        if text_changed {
            if let Some(b) = self.ivars().bubble.borrow().as_ref() {
                b.setStringValue(&NSString::from_str(&presentation.preview));
            }
            if let Some(text) = self.ivars().reply_text.borrow().as_ref() {
                text.setString(&NSString::from_str(&presentation.full));
            }
        }
        let mut walk = self.ivars().walk.borrow_mut();
        if !walk.strip_on {
            drop(walk);
            return;
        }
        walk.t += 1.0;
        let max_x = (screen.size.width
            - OverlayLayout::content_width(walk.bubble_on, &active.metrics)
            - 16.0)
            .max(16.0);
        match mood {
            Mood::Thinking | Mood::Asleep => {}
            _ => {
                walk.x += walk.dir * motion.walk_speed;
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
            Mood::Thinking => (walk.t * motion.thinking_bob_freq).sin() * motion.thinking_bob_amp,
            Mood::Talking => motion.talking_bob_offset,
            _ => ((walk.t * motion.idle_bob_freq).sin() * motion.idle_bob_amp).abs(),
        };
        let x = walk.x;
        let bubble_on = walk.bubble_on;
        let reply_expanded = walk.reply_expanded;
        let mood_changed = walk.last_mood != mood;
        walk.last_mood = mood;
        drop(walk);

        self.apply_layout(OverlayLayout::new(
            screen,
            x,
            bob,
            bubble_on,
            reply_expanded,
            reply_height,
            text_height,
            &active.metrics,
        ));
        if mood_changed {
            if let Some(c) = self.ivars().creature.borrow().as_ref() {
                c.setNeedsDisplay(true);
            }
        }

        if bubble_on {
            if let Some(b) = self.ivars().bubble.borrow().as_ref() {
                b.setHidden(reply_expanded);
            }
            if let Some(scroll) = self.ivars().reply_scroll.borrow().as_ref() {
                scroll.setHidden(!reply_expanded);
            }
            if let Some(button) = self.ivars().see_more.borrow().as_ref() {
                button.setHidden(in_flight || presentation.raw.is_empty());
                let title = if reply_expanded {
                    ns_string!("See Less")
                } else {
                    ns_string!("See More")
                };
                if !button.title().isEqualToString(title) {
                    set_reply_button_title(button, title);
                }
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
        if let Some(scroll) = self.ivars().reply_scroll.borrow().as_ref() {
            scroll.setFrame(NSRect::new(layout.scroll, layout.reply_size));
        }
        if let Some(text) = self.ivars().reply_text.borrow().as_ref() {
            text.setFrame(NSRect::new(NSPoint::ZERO, layout.text_size));
        }
        if let Some(button) = self.ivars().see_more.borrow().as_ref() {
            button.setFrameOrigin(layout.button);
        }
        if let Some(input) = self.ivars().input.borrow().as_ref() {
            input.setFrameOrigin(layout.input);
        }
    }
}

fn creature_width(metrics: &theme::Metrics) -> f64 {
    metrics.creature_h * (589.0 / 778.0)
}

fn cw_offset(metrics: &theme::Metrics) -> f64 {
    creature_width(metrics) + 8.0
}

fn ns_color(c: theme::Rgba) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(c.r, c.g, c.b, c.a)
}

fn set_reply_button_title(button: &NSButton, title: &NSString) {
    let ink = ns_color(theme::CLASSIC.palette.text_color.unwrap());
    let attrs: Retained<NSDictionary<_, AnyObject>> = NSDictionary::from_slices(
        &[unsafe { NSForegroundColorAttributeName }],
        &[ink.as_ref()],
    );
    let attributed = unsafe { NSAttributedString::new_with_attributes(title, &attrs) };
    button.setAttributedTitle(&attributed);
}

/// The theme's ink, or the system's adaptive label color when the theme
/// tracks Appearance instead of fixing one.
fn text_color(active: &theme::Theme) -> Retained<NSColor> {
    match active.palette.text_color {
        Some(c) => ns_color(c),
        None => NSColor::labelColor(),
    }
}

/// Build a client for the resolved scheme without hiding certificate rejection.
fn build_harness_client(
    endpoint: discover::Reachable,
    cert: Option<&[u8]>,
    jar: &Arc<SessionJar>,
) -> Result<Client, ClientError> {
    let origin = match endpoint {
        discover::Reachable::Http(port) => {
            return Client::with_jar(LoopbackOrigin::from_port(port), Arc::clone(jar));
        }
        discover::Reachable::Https(port) => LoopbackOrigin::from_port_https(port),
        discover::Reachable::HttpsV6(port) => {
            LoopbackOrigin::parse_https(&format!("https://[::1]:{port}"))?
        }
    };
    Client::https_with_jar(
        origin,
        cert.ok_or(ClientError::CertMismatch)?,
        Arc::clone(jar),
    )
}

/// The status worker's client, kept until the harness stops answering or its
/// pinned certificate changes, so a healthy check is one request.
struct StatusClient {
    cert: Option<Vec<u8>>,
    endpoint: discover::Reachable,
    client: Client,
}

enum HarnessProbe {
    Confirmed(discover::Reachable, Status),
    RateLimited,
    CertMismatch,
    Unavailable,
}

fn ollama_health(ollama: Option<&Ollama>) -> Health {
    match ollama
        .ok_or(OllamaError::Unreachable)
        .and_then(Ollama::tags_ok)
    {
        Ok(true) => Health::Ready {
            api_key_optional: true,
            model: ollama::MODEL.into(),
            provider: "ollama".into(),
        },
        Err(OllamaError::Unreachable) => Health::Asleep,
        Ok(false) | Err(_) => Health::Sick,
    }
}

fn harness_probe(
    cache: &mut Option<StatusClient>,
    http_fallback: &mut discover::HttpFallback,
    preferred: u16,
    home_dir: &std::path::Path,
    jar: &Arc<SessionJar>,
) -> HarnessProbe {
    let probe = harness_probe_once(cache, *http_fallback, preferred, home_dir, jar);
    match &probe {
        HarnessProbe::CertMismatch => {
            *cache = None;
            *http_fallback = discover::HttpFallback::Forbidden;
        }
        HarnessProbe::Confirmed(
            discover::Reachable::Https(_) | discover::Reachable::HttpsV6(_),
            _,
        ) => {
            *http_fallback = discover::HttpFallback::Allowed;
        }
        _ => {}
    }
    probe
}

fn harness_probe_once(
    cache: &mut Option<StatusClient>,
    http_fallback: discover::HttpFallback,
    preferred: u16,
    home_dir: &std::path::Path,
    jar: &Arc<SessionJar>,
) -> HarnessProbe {
    let cert = home::read_pinned_cert(home_dir);
    if let Some(cached) = cache.as_ref().filter(|c| c.cert == cert) {
        match cached.client.status() {
            Ok(status) => return HarnessProbe::Confirmed(cached.endpoint, status),
            Err(ClientError::RateLimited) => return HarnessProbe::RateLimited,
            Err(ClientError::CertMismatch) => return HarnessProbe::CertMismatch,
            _ => {}
        }
    }
    *cache = None;
    let reachable = match discover::resolve_reachable(preferred, cert.as_deref(), http_fallback) {
        Ok(Some(reachable)) => reachable,
        Err(ClientError::CertMismatch) => return HarnessProbe::CertMismatch,
        _ if http_fallback == discover::HttpFallback::Forbidden => {
            return HarnessProbe::CertMismatch
        }
        _ => return HarnessProbe::Unavailable,
    };
    let client = build_harness_client(reachable, cert.as_deref(), jar);
    let client = match client {
        Ok(client) => client,
        Err(ClientError::CertMismatch) => return HarnessProbe::CertMismatch,
        Err(_) => return HarnessProbe::Unavailable,
    };
    let probe = match client.status() {
        Ok(status) => HarnessProbe::Confirmed(reachable, status),
        Err(ClientError::RateLimited) => HarnessProbe::RateLimited,
        Err(ClientError::CertMismatch) => return HarnessProbe::CertMismatch,
        Err(_) => HarnessProbe::Unavailable,
    };
    *cache = Some(StatusClient {
        cert,
        endpoint: reachable,
        client,
    });
    probe
}

fn preview_text(reply: &str, in_flight: bool) -> String {
    if in_flight {
        "…thinking".into()
    } else if reply.is_empty() {
        "type below, Return to send".into()
    } else {
        display::bubble_text(reply)
    }
}

fn reply_text(reply: &str, in_flight: bool) -> String {
    if in_flight {
        "…thinking".into()
    } else if reply.is_empty() {
        "type below, Return to send".into()
    } else {
        display::expanded_text(reply)
    }
}

fn expanded_reply_heights(text: &str, metrics: &theme::Metrics) -> (f64, f64) {
    let theme::Metrics {
        reply_chars_per_line: REPLY_CHARS_PER_LINE,
        reply_line_h: REPLY_LINE_H,
        bubble_h: BUBBLE_H,
        max_expanded_reply_h: MAX_EXPANDED_REPLY_H,
        ..
    } = *metrics;
    let lines = text
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(REPLY_CHARS_PER_LINE))
        .sum::<usize>()
        .max(1);
    let full = (lines as f64 * REPLY_LINE_H + 12.0).max(BUBBLE_H);
    (full.min(MAX_EXPANDED_REPLY_H), full)
}

// ponytail: auth waits delay expiry; use the status worker if strict timing is needed.
fn expire_talking(deadline: &mut Option<Instant>, talking: &AtomicBool, now: Instant) {
    if deadline.is_some_and(|at| now >= at) {
        *deadline = None;
        talking.store(false, Ordering::SeqCst);
    }
}

fn spawn_workers(shared: Arc<Shared>) {
    let home_dir = home::default_home();
    let preferred = home::port_from_home(&home_dir);
    let status_ollama = Ollama::new().ok();
    std::thread::Builder::new()
        .name("cg-agent-status".into())
        .spawn({
            let shared = Arc::clone(&shared);
            let home_dir = home_dir.clone();
            move || {
                let mut cache = None::<StatusClient>;
                let mut http_fallback = discover::HttpFallback::Allowed;
                let mut health = Health::Asleep;
                let mut polled = None::<(u8, Instant)>;
                loop {
                    let backend = shared.backend.load(Ordering::Relaxed);
                    let due = shared.repoll.swap(false, Ordering::SeqCst)
                        || polled.is_none_or(|(b, at)| b != backend || at.elapsed() >= POLL_EVERY);
                    if due {
                        if polled.is_some_and(|(previous, _)| previous != backend) {
                            health = Health::Asleep;
                        }
                        health = if backend == BACKEND_OLLAMA {
                            ollama_health(status_ollama.as_ref())
                        } else {
                            let generation = shared.guide.lock().unwrap().generation();
                            let probe = harness_probe(&mut cache, &mut http_fallback, preferred, &home_dir, &shared.jar);
                            let mut guide = shared.guide.lock().unwrap();
                            if !guide.current(generation) {
                                Health::Asleep
                            } else {
                                let next = match probe {
                                    HarnessProbe::Confirmed(endpoint, status) => {
                                        let changed =
                                            guide.observe(generation, Some((endpoint, &status)));
                                        if changed {
                                            *shared.last_reply.lock().unwrap() =
                                                guide.phase().guidance().into();
                                        }
                                        if matches!(guide.phase(), Phase::Ready(_)) {
                                            Health::Ready {
                                                api_key_optional: status.api_key_optional,
                                                model: status.model,
                                                provider: status.provider,
                                            }
                                        } else {
                                            Health::Sick
                                        }
                                    }
                                    HarnessProbe::CertMismatch => {
                                        guide.observe(generation, None);
                                        *shared.last_reply.lock().unwrap() =
                                            "harness certificate changed — verify it, then re-check trust".into();
                                        Health::Sick
                                    }
                                    HarnessProbe::RateLimited => health.clone(),
                                    HarnessProbe::Unavailable => {
                                        if guide.observe(generation, None) {
                                            *shared.last_reply.lock().unwrap() =
                                                guide.phase().guidance().into();
                                        }
                                        Health::Asleep
                                    }
                                };
                                next
                            }
                        };
                        polled = Some((backend, Instant::now()));
                    }
                    let input = health.input(
                        shared.in_flight.load(Ordering::Relaxed),
                        shared.talking.load(Ordering::Relaxed),
                    );
                    *shared.mood.lock().unwrap_or_else(PoisonError::into_inner) = mood::mood(input);
                    std::thread::sleep(MOOD_EVERY);
                }
            }
        })
        .expect("status thread");

    std::thread::Builder::new()
        .name("cg-agent-chat".into())
        .spawn({
            let shared = Arc::clone(&shared);
            let home_dir = home_dir.clone();
            move || {
                let mut last_endpoint = None;
                let mut harness = None::<Client>;
                let ollama = Ollama::new().ok();
                let mut session = None::<String>;
                let mut talking_deadline = None;
                loop {
                    expire_talking(&mut talking_deadline, &shared.talking, Instant::now());
                    let password_change = {
                        shared
                            .pending_password_change
                            .lock()
                            .unwrap()
                            .take()
                    };
                    if let Some((generation, current_password, password)) = password_change {
                        let endpoint = shared.guide.lock().unwrap().reset_endpoint(generation);
                        if let Some(endpoint) = endpoint {
                            if last_endpoint != Some(endpoint) || harness.is_none() {
                                let cert = home::read_pinned_cert(&home_dir);
                                harness = build_harness_client(endpoint, cert.as_deref(), &shared.jar).ok();
                                session = None;
                                last_endpoint = Some(endpoint);
                            }
                            let result = harness.as_mut().ok_or(ClientError::Unreachable)
                                .and_then(|client| client.change_password(&current_password, &password));
                            let mut guide = shared.guide.lock().unwrap();
                            if guide.current(generation) {
                                match result {
                                    Ok(()) if guide.password_changed(generation) => {
                                        session = None;
                                        shared.repoll.store(true, Ordering::SeqCst);
                                        *shared.last_reply.lock().unwrap() =
                                            "password changed — verifying Harness…".into();
                                    }
                                    Err(e) => {
                                        *shared.last_reply.lock().unwrap() =
                                            format!("password reset failed: {e}");
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    let login = { shared.pending_login.lock().unwrap().take() };
                    if let Some((generation, username, password)) = login {
                        let endpoint = shared.guide.lock().unwrap().endpoint(generation);
                        if let Some(endpoint) = endpoint {
                            if last_endpoint != Some(endpoint) || harness.is_none() {
                                let cert = home::read_pinned_cert(&home_dir);
                                harness = build_harness_client(endpoint, cert.as_deref(), &shared.jar).ok();
                                session = None;
                                last_endpoint = Some(endpoint);
                            }
                            let result = harness.as_mut().ok_or(ClientError::Unreachable)
                                .and_then(|client| client.login(&username, &password));
                            let mut guide = shared.guide.lock().unwrap();
                            if guide.current(generation) {
                                match result {
                                    Ok(info) if guide.login_result(generation, info.must_change_password) => {
                                        session = None;
                                        if !info.must_change_password {
                                            shared.repoll.store(true, Ordering::SeqCst);
                                        }
                                        *shared.last_reply.lock().unwrap() = if info.must_change_password {
                                            guide.phase().guidance().into()
                                        } else {
                                            format!("logged in as {} — verifying Harness…", info.username)
                                        };
                                    }
                                    Err(e) => {
                                        *shared.last_reply.lock().unwrap() = format!("login failed: {e}");
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    let msg = { shared.pending.lock().unwrap().take() };
                    if let Some((backend, generation, message)) = msg {
                        if shared.backend.load(Ordering::SeqCst) != backend
                            || shared.guide.lock().unwrap().generation() != generation
                        {
                            continue;
                        }
                        let endpoint = if backend == BACKEND_HARNESS {
                            let guide = shared.guide.lock().unwrap();
                            let Some(endpoint) = guide.ready_endpoint(generation) else {
                                *shared.last_reply.lock().unwrap() = guide.phase().guidance().into();
                                continue;
                            };
                            Some(endpoint)
                        } else {
                            None
                        };
                        shared.in_flight.store(true, Ordering::SeqCst);
                        talking_deadline = None;
                        shared.talking.store(false, Ordering::SeqCst);
                        if backend == BACKEND_OLLAMA {
                            let result = ollama
                                .as_ref()
                                .ok_or(OllamaError::Unreachable)
                                .and_then(|o| o.chat(&message));
                            if shared.backend.load(Ordering::SeqCst) != backend
                                || shared.guide.lock().unwrap().generation() != generation
                            {
                                shared.in_flight.store(false, Ordering::SeqCst);
                                continue;
                            }
                            match result {
                                Ok(reply) => {
                                    *shared.last_reply.lock().unwrap() = reply;
                                    talking_deadline = Some(Instant::now() + Duration::from_secs(8));
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
                                let endpoint = endpoint.unwrap();
                                if last_endpoint != Some(endpoint) || harness.is_none() {
                                    let cert = home::read_pinned_cert(&home_dir);
                                    harness = build_harness_client(endpoint, cert.as_deref(), &shared.jar).ok();
                                    session = None;
                                    last_endpoint = Some(endpoint);
                                }
                                let client = harness.as_mut().ok_or(ClientError::Unreachable)?;
                                if session.is_none() {
                                    session = Some(client.ensure_session()?);
                                }
                                match client.chat(session.as_deref().unwrap(), &message) {
                                    // Cleared in the console or owned by another
                                    // account now: pick up (or create) ours, once.
                                    Err(ClientError::SessionGone) => {
                                        let id = session.insert(client.ensure_session()?);
                                        client.chat(id, &message)
                                    }
                                    other => other,
                                }
                            })();
                            // Rebuild next time: the harness may have restarted
                            // elsewhere or renewed its certificate.
                            if matches!(
                                result,
                                Err(ClientError::Unreachable | ClientError::CertMismatch)
                            ) {
                                harness = None;
                            }
                            let mut guide = shared.guide.lock().unwrap();
                            if !guide.current(generation) {
                                shared.in_flight.store(false, Ordering::SeqCst);
                                continue;
                            }
                            match result {
                                Ok(reply) => {
                                    let mut text = reply.reply;
                                    if !reply.web_tools.is_empty() {
                                        text = display::with_web_tools_note(&text, reply.web_tools.len());
                                    }
                                    *shared.last_reply.lock().unwrap() = text;
                                    talking_deadline = Some(Instant::now() + Duration::from_secs(8));
                                    shared.talking.store(true, Ordering::SeqCst);
                                }
                                Err(ClientError::RateLimited) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "harness is rate limiting — wait a few seconds".into();
                                }
                                Err(ClientError::ChatBusy) => {
                                    *shared.last_reply.lock().unwrap() =
                                        "busy — wait a beat".into();
                                }
                                Err(ClientError::LoginRequired) => {
                                    guide.auth_required(generation, false);
                                    *shared.last_reply.lock().unwrap() = guide.phase().guidance().into();
                                }
                                Err(ClientError::PasswordChangeRequired) => {
                                    guide.auth_required(generation, true);
                                    *shared.last_reply.lock().unwrap() = guide.phase().guidance().into();
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
        pending_password_change: Mutex::new(None),
        in_flight: AtomicBool::new(false),
        talking: AtomicBool::new(false),
        click: AtomicBool::new(false),
        backend: AtomicU8::new(DEFAULT_BACKEND),
        guide: Mutex::new(Guide::default()),
        jar: Arc::default(),
        repoll: AtomicBool::new(false),
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
    fn newer_reply_keeps_talking_until_its_own_deadline() {
        let start = Instant::now();
        let talking = AtomicBool::new(true);
        let mut deadline = Some(start + Duration::from_secs(8));
        expire_talking(&mut deadline, &talking, start + Duration::from_secs(7));
        assert!(talking.load(Ordering::SeqCst));
        deadline = Some(start + Duration::from_secs(15));
        expire_talking(&mut deadline, &talking, start + Duration::from_secs(8));
        assert!(talking.load(Ordering::SeqCst));
        expire_talking(&mut deadline, &talking, start + Duration::from_secs(15));
        assert!(!talking.load(Ordering::SeqCst));
        assert!(deadline.is_none());
    }

    #[test]
    fn certificate_rejection_survives_missing_cert_on_later_poll() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("tls")).unwrap();
        let cert_path = home.path().join("tls/server.pem");
        std::fs::write(
            &cert_path,
            "-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let mut server = mockito::Server::new();
        let request = server.mock("GET", "/api/status").expect(0).create();
        let port = url::Url::parse(&server.url()).unwrap().port().unwrap();
        let mut cache = None;
        let mut fallback = discover::HttpFallback::Allowed;
        let jar = Arc::default();
        assert!(matches!(
            harness_probe(&mut cache, &mut fallback, port, home.path(), &jar),
            HarnessProbe::CertMismatch
        ));
        assert_eq!(fallback, discover::HttpFallback::Forbidden);
        assert!(cache.is_none());
        std::fs::remove_file(cert_path).unwrap();
        assert!(matches!(
            harness_probe(&mut cache, &mut fallback, port, home.path(), &jar),
            HarnessProbe::CertMismatch
        ));
        assert_eq!(fallback, discover::HttpFallback::Forbidden);
        assert!(cache.is_none());
        request.assert();
    }

    #[test]
    fn ipv6_discovery_client_keeps_the_confirmed_address() {
        let cert = b"-----BEGIN CERTIFICATE-----
MIIDQDCCAiigAwIBAgIUMDtxYF827xGvesjv4aFLv9ZggHQwDQYJKoZIhvcNAQEL
BQAwFDESMBAGA1UEAwwJMTI3LjAuMC4xMB4XDTI2MDkyODA0NTgyM1oXDTI2MDkz
MDA0NTgyM1owFDESMBAGA1UEAwwJMTI3LjAuMC4xMIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEAtx90E9cLXevCuzJlXkBXTPxbkuFkCcWw17WtHpkqE+ax
a8T1miE/MI8tS2cmzEWMEsBVch1rSFK9zjvxkBjSzUl9c0UjdWV7ZBRnPrwKUxp6
rJKjhnBQV2PdMu/aN75TinbQ2zrZwxF3z7YyuYzJF51JhUJwLgEk5dHja6/JcAlS
3DvrYkX+reHWIy7gEjrmnmIKIGZLSFns7gNCX/6u3M/A1MFMCf3XHPtgCdlobLZD
KfjtU/r0lC6SPJOQjxMxHUz+npjXZm8KP5Fqr7QdL0MbnoXctRbcYfLzBIzqgOg5
hLyziukAhKukDEiWxFAr+XDsXGix4P/suzpFXurKRQIDAQABo4GJMIGGMB0GA1Ud
DgQWBBRkBEC+lim2enZdrkBUZP3pAN0KxzAfBgNVHSMEGDAWgBRkBEC+lim2enZd
rkBUZP3pAN0KxzAhBgNVHREEGjAYhwR/AAABhxAAAAAAAAAAAAAAAAAAAAABMAwG
A1UdEwEB/wQCMAAwEwYDVR0lBAwwCgYIKwYBBQUHAwEwDQYJKoZIhvcNAQELBQAD
ggEBAHauA2ZHhmy1oPcBTW9IQ/m58RJikYBQAEyDRXoTmNo2HvsVtLi0VuBP3F4M
acma09lv7uJ5F/KjbM83pH5rGHUga3+Fm4/I5czFv58Exd6uNPsM18+BWrAsDg1v
nFYebmD1Ass8QhA9teyrZN3B33DXplUnIL1O/jh5nQTBbhJ6zYGnc0His67zrLwg
pdXGPUOUoW4Iq3GjnZmGmV49C57yRSrtaic/Lq94YIjbZXQbnHr375ZBQLkzivTq
dCutk0xvqDJOMQPzR8ekTw8sktOTFh8y14QdaI93uPOTudcG0Sp0PlxXiQncElcc
a+VhHbrWdvoCoROeKF3msYiqjFE=
-----END CERTIFICATE-----
";
        let client = build_harness_client(
            discover::Reachable::HttpsV6(8790),
            Some(cert),
            &Arc::default(),
        )
        .unwrap();
        assert_eq!(client.origin().as_str(), "https://[::1]:8790");
    }

    #[test]
    fn direct_ollama_is_the_launch_default() {
        assert_eq!(DEFAULT_BACKEND, BACKEND_OLLAMA);
    }

    #[test]
    fn panel_is_only_as_large_as_the_interactive_views() {
        // Pinned to the classic theme's own numbers via an explicit
        // `&theme::CLASSIC.metrics` argument (not `theme::active()`), so
        // this stays a fixed characterization of that theme no matter what
        // `CG_AGENT_THEME` is set to in the test environment.
        let metrics = theme::CLASSIC.metrics;
        let theme::Metrics {
            bubble_w: BUBBLE_W,
            bubble_h: BUBBLE_H,
            creature_h: CREATURE_H,
            ..
        } = metrics;
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let idle = OverlayLayout::new(
            screen, 100.0, 0.0, false, false, BUBBLE_H, BUBBLE_H, &metrics,
        );
        assert_eq!(idle.panel.size.width, creature_width(&metrics));
        assert_eq!(idle.panel.size.height, CREATURE_H);

        let talk = OverlayLayout::new(
            screen, 100.0, 8.0, true, false, BUBBLE_H, BUBBLE_H, &metrics,
        );
        assert_eq!(talk.panel.size.width, BUBBLE_W);
        assert_eq!(talk.panel.size.height, BUBBLE_H + CREATURE_H + 8.0);
        assert_eq!(talk.creature.x, 0.0);
        assert!(talk.input.y >= 0.0);
        assert!(talk.bubble.y + BUBBLE_H <= talk.panel.size.height);
    }

    #[test]
    fn expanded_reply_grows_then_scrolls() {
        let metrics = theme::CLASSIC.metrics;
        let theme::Metrics {
            bubble_h: BUBBLE_H,
            max_expanded_reply_h: MAX_EXPANDED_REPLY_H,
            ..
        } = metrics;
        let (short_visible, short_full) = expanded_reply_heights("short reply", &metrics);
        assert_eq!(short_visible, BUBBLE_H);
        assert_eq!(short_full, BUBBLE_H);

        let (visible, full) = expanded_reply_heights(&"x".repeat(8_000), &metrics);
        assert_eq!(visible, MAX_EXPANDED_REPLY_H);
        assert!(full > visible);
    }

    #[test]
    fn both_backends_share_the_same_reply_renderer() {
        assert_eq!(reply_text("harness reply", false), "harness reply");
        assert_eq!(reply_text("ollama reply", false), "ollama reply");
    }

    #[test]
    fn unchanged_reply_does_not_replace_native_text_on_animation_ticks() {
        let metrics = theme::CLASSIC.metrics;
        let mut presentation = ReplyPresentation::new(&metrics);
        assert_eq!(presentation.preview, "type below, Return to send");
        assert!(presentation.update("a completed reply", false, &metrics));
        for _ in 0..60 {
            assert!(!presentation.update("a completed reply", false, &metrics));
        }
        assert_eq!(presentation.full, "a completed reply");

        // A repeated answer still needs rendering when a thinking turn ends.
        assert!(presentation.update("a completed reply", true, &metrics));
        assert_eq!(presentation.full, "…thinking");
        assert!(presentation.update("a completed reply", false, &metrics));
        assert_eq!(presentation.full, "a completed reply");
        assert!(presentation.update("", false, &metrics));
        assert_eq!(presentation.full, "type below, Return to send");
    }

    #[test]
    fn changed_replies_refresh_sanitization_limits_and_height_for_both_themes() {
        for metrics in [theme::CLASSIC.metrics, theme::FABLE_PROTOCOL.metrics] {
            let mut presentation = ReplyPresentation::new(&metrics);
            let raw = format!("\u{1b}[31m{}\0", "界".repeat(9_000));
            assert!(presentation.update(&raw, false, &metrics));
            assert_eq!(presentation.preview.chars().count(), 401);
            assert_eq!(presentation.full.chars().count(), 8_001);
            assert!(!presentation.full.contains('\u{1b}'));
            assert!(!presentation.full.contains('\0'));
            assert_eq!(presentation.heights.0, metrics.max_expanded_reply_h);
            assert!(presentation.heights.1 > presentation.heights.0);

            assert!(presentation.update("short", false, &metrics));
            assert_eq!(presentation.full, "short");
            assert_eq!(presentation.heights, (metrics.bubble_h, metrics.bubble_h));
        }
    }

    #[test]
    fn fable_protocol_lays_out_differently_from_classic() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let for_theme = |metrics: &theme::Metrics| {
            OverlayLayout::new(
                screen,
                100.0,
                0.0,
                true,
                false,
                metrics.bubble_h,
                metrics.bubble_h,
                metrics,
            )
        };
        let classic = for_theme(&theme::CLASSIC.metrics);
        let fable = for_theme(&theme::FABLE_PROTOCOL.metrics);
        assert_ne!(classic.panel.size.width, fable.panel.size.width);
        assert_ne!(classic.panel.size.height, fable.panel.size.height);
    }
}
