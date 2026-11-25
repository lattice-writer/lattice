//! Native X11 input for the scripted runs: the same calls as the macOS
//! `native` module and `native_windows.rs`, made with the XTEST extension
//! on a connection of the scenario's own.
//!
//! GPUI's X11 client reads the pointer through XInput 2, whose events a
//! client cannot send (`SendEvent` carries core events only), so the input
//! goes through XTEST: the server takes each fake press or motion as if a
//! device made it and delivers it, XInput 2 included, to the window under
//! the pointer or with the keyboard focus. Nothing reaches GPUI before its
//! own event loop reads the socket, so, as with Windows' posted keys, a key
//! is handled once the scenario has waited `QUEUE_SETTLE`; `send_mouse` and
//! the rest wait for the server to have taken the events (a round trip),
//! and the scenario's pauses after them let GPUI read them.
//!
//! The runner (`xvfb-run` in gpui.yml) has the display to itself and no
//! window manager, so a window takes the keyboard by asking for it and
//! nothing moves or decorates it.

use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use gpui::{Pixels, Point, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{
    connection::{Connection, RequestConnection as _},
    protocol::{
        xproto::{
            self, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask,
            ImageFormat, ImageOrder, InputFocus, StackMode,
        },
        xtest::{self, ConnectionExt as _},
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

/// How long GPUI's event loop gets to read a key the server delivered and
/// handle it.
pub const QUEUE_SETTLE: Duration = Duration::from_millis(50);

/// The → key, spelled as the macOS runs spell it, so a scenario sends it
/// the same way everywhere.
pub const RIGHT_ARROW: char = '\u{f703}';

/// X keysyms the scenario's keys need beyond the Latin-1 ones, which equal
/// their characters (`X11/keysymdef.h`).
const XK_BACKSPACE: u32 = 0xff08;
const XK_TAB: u32 = 0xff09;
const XK_RETURN: u32 = 0xff0d;
const XK_ESCAPE: u32 = 0xff1b;
const XK_RIGHT: u32 = 0xff53;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_CONTROL_L: u32 = 0xffe3;
/// Keysyms for the rest of Unicode are the code point plus this.
const XK_UNICODE: u32 = 0x0100_0000;

/// Core pointer buttons: left, right, and the wheel's two directions.
const BUTTON_LEFT: u8 = 1;
const BUTTON_RIGHT: u8 = 3;
const WHEEL_UP: u8 = 4;
const WHEEL_DOWN: u8 = 5;
/// A wheel notch, in the units `send_wheel` is given (as on Windows).
const WHEEL_NOTCH: f32 = 120.;

/// The scenario's own connection to the display GPUI draws on, and what it
/// read of the server once.
struct Display {
    connection: RustConnection,
    root: xproto::Window,
    min_keycode: u8,
    keysyms_per_keycode: usize,
    keysyms: Vec<u32>,
}

static DISPLAY: OnceLock<Display> = OnceLock::new();

/// The connection, opened on first use. The scenario runs on GPUI's main
/// thread alone, so two first uses never race.
fn display() -> Result<&'static Display> {
    if let Some(display) = DISPLAY.get() {
        return Ok(display);
    }
    let (connection, screen) = x11rb::connect(None).context("connecting to the X display")?;
    connection
        .extension_information(xtest::X11_EXTENSION_NAME)?
        .context("the X server has no XTEST extension")?;
    let setup = connection.setup();
    let root = setup.roots[screen].root;
    let min_keycode = setup.min_keycode;
    let count = setup.max_keycode - min_keycode + 1;
    let mapping = connection
        .get_keyboard_mapping(min_keycode, count)?
        .reply()?;
    Ok(DISPLAY.get_or_init(|| Display {
        connection,
        root,
        min_keycode,
        keysyms_per_keycode: usize::from(mapping.keysyms_per_keycode),
        keysyms: mapping.keysyms,
    }))
}

impl Display {
    /// The key that types `keysym`, and whether it needs Shift: the first
    /// keycode with it in the unshifted or shifted column.
    fn key(&self, keysym: u32) -> Option<(u8, bool)> {
        self.keysyms
            .chunks(self.keysyms_per_keycode.max(1))
            .enumerate()
            .find_map(|(index, row)| {
                let column = row.iter().take(2).position(|sym| *sym == keysym)?;
                let keycode = u8::try_from(usize::from(self.min_keycode) + index).ok()?;
                Some((keycode, column == 1))
            })
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<()> {
        self.connection
            .xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, x, y, 0)?;
        Ok(())
    }

    /// Waits until the server has handled every request sent so far, so the
    /// events they made are on their way to GPUI.
    fn settle(&self) -> Result<()> {
        self.connection.sync()?;
        Ok(())
    }

    /// `position` in `native`'s window, on the root window, in device pixels.
    fn on_root(&self, native: Native, position: Point<Pixels>) -> Result<(i16, i16)> {
        let x = (f32::from(position.x) * native.scale).round() as i16;
        let y = (f32::from(position.y) * native.scale).round() as i16;
        let reply = self
            .connection
            .translate_coordinates(native.window, self.root, x, y)?
            .reply()?;
        Ok((reply.dst_x, reply.dst_y))
    }

    /// Gives `window` the keyboard, unless it has it already: without a
    /// window manager, nothing else would.
    fn focus(&self, window: xproto::Window) -> Result<()> {
        if self.connection.get_input_focus()?.reply()?.focus != window {
            self.connection
                .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)?;
        }
        Ok(())
    }
}

/// The X window GPUI draws into, and its scale factor. Valid while the GPUI
/// window is open.
#[derive(Clone, Copy)]
pub struct Native {
    window: xproto::Window,
    scale: f32,
}

pub fn native(window: &Window) -> Result<Native> {
    let handle = HasWindowHandle::window_handle(window).map_err(|error| anyhow!("{error}"))?;
    let window_id = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window)?,
        _ => return Err(anyhow!("not an X11 window (the scenario runs on X11 only)")),
    };
    Ok(Native {
        window: window_id,
        scale: window.scale_factor(),
    })
}

/// Presses and releases the key for `character`, with Ctrl held when
/// `command` (the shortcut modifier off macOS, where it is ⌘). The window
/// takes the keyboard first. Wait `QUEUE_SETTLE` before reading what the
/// key did.
pub fn send_key(native: Native, character: char, command: bool) -> Result<()> {
    let display = display()?;
    let keysym = match character {
        '\u{1b}' => XK_ESCAPE,
        '\n' | '\r' => XK_RETURN,
        '\t' => XK_TAB,
        '\u{8}' | '\u{7f}' => XK_BACKSPACE,
        RIGHT_ARROW => XK_RIGHT,
        ' '..='~' | '\u{a0}'..='\u{ff}' => u32::from(character),
        _ => XK_UNICODE + u32::from(character),
    };
    let (keycode, shift) = display
        .key(keysym)
        .ok_or_else(|| anyhow!("{character:?} has no key on this keyboard map"))?;
    let mut keys = Vec::with_capacity(3);
    if command {
        keys.push(display.key(XK_CONTROL_L).context("no Control key")?.0);
    }
    if shift {
        keys.push(display.key(XK_SHIFT_L).context("no Shift key")?.0);
    }
    keys.push(keycode);

    display.focus(native.window)?;
    for key in &keys {
        display.fake(xproto::KEY_PRESS_EVENT, *key, 0, 0)?;
    }
    for key in keys.iter().rev() {
        display.fake(xproto::KEY_RELEASE_EVENT, *key, 0, 0)?;
    }
    display.settle()
}

/// `send_key` releases its modifiers with the key, so there is nothing
/// left to lift; kept for the scenario's Windows-shaped key path.
pub fn release_keys() -> Result<()> {
    Ok(())
}

pub enum Mouse {
    Down,
    Up,
    /// The right button, which opens a context menu.
    RightDown,
    RightUp,
    /// The pointer moving with no button held, as for a hover.
    Moved,
    Dragged,
}

/// The pointer moved to `position` in window coordinates, then the button
/// event `kind` names there.
pub fn send_mouse(native: Native, kind: Mouse, position: Point<Pixels>) -> Result<()> {
    let display = display()?;
    let (x, y) = display.on_root(native, position)?;
    // A motion's detail 0 is an absolute position on the root window.
    display.fake(xproto::MOTION_NOTIFY_EVENT, 0, x, y)?;
    let button = match kind {
        Mouse::Down => Some((xproto::BUTTON_PRESS_EVENT, BUTTON_LEFT)),
        Mouse::Up => Some((xproto::BUTTON_RELEASE_EVENT, BUTTON_LEFT)),
        Mouse::RightDown => Some((xproto::BUTTON_PRESS_EVENT, BUTTON_RIGHT)),
        Mouse::RightUp => Some((xproto::BUTTON_RELEASE_EVENT, BUTTON_RIGHT)),
        Mouse::Moved | Mouse::Dragged => None,
    };
    if let Some((event, button)) = button {
        display.fake(event, button, x, y)?;
    }
    display.settle()
}

/// Wheel turns at `position` in window coordinates, `dy` negative scrolling
/// the content down as on macOS. `dy` counts 120 to a notch, as on Windows,
/// and each notch is a press of the wheel's button, which GPUI scrolls a
/// few lines for; the scenarios ask only that the text moved.
pub fn send_wheel(native: Native, position: Point<Pixels>, dy: i32) -> Result<()> {
    let display = display()?;
    let (x, y) = display.on_root(native, position)?;
    display.fake(xproto::MOTION_NOTIFY_EVENT, 0, x, y)?;
    let button = if dy < 0 { WHEEL_DOWN } else { WHEEL_UP };
    let notches = (dy.unsigned_abs() as f32 / WHEEL_NOTCH).round().max(1.) as u32;
    for _ in 0..notches {
        display.fake(xproto::BUTTON_PRESS_EVENT, button, x, y)?;
        display.fake(xproto::BUTTON_RELEASE_EVENT, button, x, y)?;
    }
    display.settle()
}

/// A left click at `position`.
pub fn send_click(native: Native, position: Point<Pixels>) -> Result<()> {
    send_mouse(native, Mouse::Down, position)?;
    send_mouse(native, Mouse::Up, position)
}

/// A left click on a control in the title bar: the title bar is GPUI's own
/// drawing here, so this is a plain click.
pub fn send_titlebar_click(native: Native, position: Point<Pixels>) -> Result<()> {
    send_click(native, position)
}

/// A right click at `position`, which opens a context menu.
pub fn send_right_click(native: Native, position: Point<Pixels>) -> Result<()> {
    send_mouse(native, Mouse::RightDown, position)?;
    send_mouse(native, Mouse::RightUp, position)
}

/// A left double click at `position`: GPUI counts two presses close in time
/// and place as one double click.
pub fn send_double_click(native: Native, position: Point<Pixels>) -> Result<()> {
    for _ in 0..2 {
        send_mouse(native, Mouse::Down, position)?;
        send_mouse(native, Mouse::Up, position)?;
    }
    Ok(())
}

/// Close the window as a window manager's close button does: the
/// `WM_DELETE_WINDOW` message, which GPUI hands to the window's should-close
/// handler before destroying it.
pub fn request_close(native: Native) {
    let sent = display().and_then(|display| {
        let connection = &display.connection;
        let protocols = connection
            .intern_atom(false, b"WM_PROTOCOLS")?
            .reply()?
            .atom;
        let delete = connection
            .intern_atom(false, b"WM_DELETE_WINDOW")?
            .reply()?
            .atom;
        let message = ClientMessageEvent::new(
            32,
            native.window,
            protocols,
            [delete, x11rb::CURRENT_TIME, 0, 0, 0],
        );
        connection.send_event(false, native.window, EventMask::NO_EVENT, message)?;
        display.settle()
    });
    if let Err(error) = sent {
        log::warn!("could not ask the window to close: {error:#}");
    }
}

/// Raises the window above the others and gives it the keyboard.
pub fn bring_to_front(native: Native) {
    let raised = display().and_then(|display| {
        raise(display, native)?;
        display.focus(native.window)?;
        display.settle()
    });
    if let Err(error) = raised {
        log::warn!("could not bring the window to the front: {error:#}");
    }
}

/// Raises the window above the others, leaving the keyboard where it is.
pub fn order_front(native: Native) {
    let raised = display().and_then(|display| {
        raise(display, native)?;
        display.settle()
    });
    if let Err(error) = raised {
        log::warn!("could not raise the window: {error:#}");
    }
}

fn raise(display: &Display, native: Native) -> Result<()> {
    display.connection.configure_window(
        native.window,
        &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
    )?;
    Ok(())
}

/// The window's pixels as the X server holds them, fully opaque: GPUI draws
/// into an ARGB window, whose alpha a look review has no use for.
pub fn capture_client(native: Native) -> Result<image::RgbaImage> {
    let display = display()?;
    let connection = &display.connection;
    let geometry = connection.get_geometry(native.window)?.reply()?;
    let reply = connection
        .get_image(
            ImageFormat::Z_PIXMAP,
            native.window,
            0,
            0,
            geometry.width,
            geometry.height,
            u32::MAX,
        )?
        .reply()?;
    let setup = connection.setup();
    let bits_per_pixel = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == reply.depth)
        .map(|format| format.bits_per_pixel);
    if bits_per_pixel != Some(32) {
        return Err(anyhow!(
            "the window's depth {} is not 32 bits a pixel ({bits_per_pixel:?})",
            reply.depth
        ));
    }
    let visual = setup
        .roots
        .iter()
        .flat_map(|screen| &screen.allowed_depths)
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.visual_id == reply.visual)
        .context("the window's visual is not on the display")?;
    let channel = |pixel: u32, mask: u32| ((pixel & mask) >> mask.trailing_zeros()) as u8;
    let big_endian = setup.image_byte_order == ImageOrder::MSB_FIRST;
    let mut rgba = Vec::with_capacity(reply.data.len());
    for bytes in reply.data.chunks_exact(4) {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let pixel = if big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        };
        rgba.extend_from_slice(&[
            channel(pixel, visual.red_mask),
            channel(pixel, visual.green_mask),
            channel(pixel, visual.blue_mask),
            u8::MAX,
        ]);
    }
    image::RgbaImage::from_raw(u32::from(geometry.width), u32::from(geometry.height), rgba)
        .context("the window's image is shorter than its size")
}

/// The scenario's foreground sections (`scenario/input_lock.rs`) matter on
/// macOS only; the Linux runner has its display to itself, so the app never
/// reads as active outside one.
pub fn app_is_active() -> bool {
    false
}

/// See `app_is_active`.
pub fn deactivate_app() {}

/// The runner has the display, and its clipboard, to itself.
pub fn release_private_pasteboard() {}

/// macOS opts out of App Nap here; X11 does not throttle the window.
pub fn begin_latency_critical_activity() {}
