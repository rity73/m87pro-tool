//! Win32 window: controls, layout, messages and background sends.
//!
//! The model (`App`) and the controls (`Ui`) live in one thread-local
//! cell. Handlers borrow it briefly; anything modal (message boxes, color
//! and file dialogs) runs after the borrow ends, so nested message loops
//! never find the cell borrowed.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use m87_core::state;
use m87_hid::{list_interfaces, Error as HidError, Keyboard, Role};
use m87_proto::keymap::{modifier, KeyAction, Layer, Mouse, MEDIA};
use m87_proto::lighting::{Effect, Rgb, MAX_LEVEL};
use m87_proto::settings::SLEEP_CHOICES;
use m87_proto::Link;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::{
    ChooseColorW, GetOpenFileNameW, GetSaveFileNameW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, DRAWITEMSTRUCT, ICC_BAR_CLASSES, ICC_STANDARD_CLASSES, ICC_TAB_CLASSES, INITCOMMONCONTROLSEX,
    MEASUREITEMSTRUCT, NMHDR, NMTTDISPINFOW, ODT_LISTBOX, TOOLTIP_FLAGS, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetCapture, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{App, Job, Packets, Page, Tone, PALETTE};
use crate::i18n::{effect_name, media_name, mouse_name, system_lang, tr, Lang, T};
use crate::kbview::{self, Mode};
use crate::theme::{self, glyph, Look};

// ---------------------------------------------------------------- constants

/// Design size of the client area, in 96-DPI pixels.
const CLIENT_W: i32 = 1140;
const CLIENT_H: i32 = 680;

/// Page controls are laid out from the old single-column design and moved
/// right by this much to make room for the navigation column.
const CONTENT_DX: i32 = 160;
/// White card behind the page controls: left, top, right, bottom.
const CARD: [i32; 4] = [168, 64, 1132, 612];

const WM_APP_DONE: u32 = WM_APP + 1;
const WM_APP_BATTERY: u32 = WM_APP + 2;
const WM_APP_KEY: u32 = WM_APP + 3;

const TIMER_SCAN: usize = 1;
const TIMER_BATTERY: usize = 2;

// Control styles and messages (plain numbers keep the types simple).
const BS_AUTOCHECKBOX: u32 = 0x3;
const BS_AUTORADIOBUTTON: u32 = 0x9;
const BS_OWNERDRAW: u32 = 0xB;
const SS_LEFT: u32 = 0x0;
const SS_NOPREFIX: u32 = 0x80;
const ES_MULTILINE: u32 = 0x4;
const ES_AUTOVSCROLL: u32 = 0x40;
const ES_AUTOHSCROLL: u32 = 0x80;
const LBS_NOTIFY: u32 = 0x1;
const LBS_NOINTEGRALHEIGHT: u32 = 0x100;
const CBS_DROPDOWNLIST: u32 = 0x3;
const TBS_AUTOTICKS: u32 = 0x1;
const BM_GETCHECK: u32 = 0xF0;
const BM_SETCHECK: u32 = 0xF1;
const LB_ADDSTRING: u32 = 0x180;
const LB_RESETCONTENT: u32 = 0x184;
const LB_SETCURSEL: u32 = 0x186;
const LB_GETCURSEL: u32 = 0x188;
const CB_ADDSTRING: u32 = 0x143;
const CB_GETCURSEL: u32 = 0x147;
const CB_RESETCONTENT: u32 = 0x14B;
const CB_SETCURSEL: u32 = 0x14E;
const CB_GETDROPPEDSTATE: u32 = 0x157;
const TBM_GETPOS: u32 = 0x400;
const TBM_SETPOS: u32 = 0x405;
const TBM_SETRANGEMIN: u32 = 0x407;
const TBM_SETRANGEMAX: u32 = 0x408;
const LBS_OWNERDRAWFIXED: u32 = 0x10;
const LBS_HASSTRINGS: u32 = 0x40;
const LB_GETTEXT: u32 = 0x189;
const LB_GETTEXTLEN: u32 = 0x18A;
const LB_SETITEMHEIGHT: u32 = 0x1A0;
const TTS_ALWAYSTIP: u32 = 0x1;
const TTS_NOPREFIX: u32 = 0x2;
const TTF_IDISHWND: u32 = 0x1;
const TTF_SUBCLASS: u32 = 0x10;
const TTM_ADDTOOLW: u32 = WM_USER + 50;
const TTN_GETDISPINFOW: u32 = (-530i32) as u32;
/// Height of a list box row, in 96-DPI pixels.
const LIST_ROW: i32 = 30;
const BN_CLICKED: u32 = 0;
const LBN_SELCHANGE: u32 = 1;
const CBN_SELCHANGE: u32 = 1;
const MK_LBUTTON: usize = 0x1;
const IDOK_CMD: u16 = 1;
const IDCANCEL_CMD: u16 = 2;

// ---------------------------------------------------------------- control ids

mod id {
    /// Navigation items, one per page (300..=306).
    pub const NAV: u16 = 300;
    pub const TITLE: u16 = 310;
    pub const CONN: u16 = 101;
    pub const LANG_LBL: u16 = 102;
    pub const LANG: u16 = 103;
    pub const STATUS: u16 = 104;

    pub const L_EFFECT_LBL: u16 = 110;
    pub const L_EFFECTS: u16 = 111;
    pub const L_COLOR_LBL: u16 = 112;
    pub const L_PALETTE: u16 = 120; // 120..=128
    pub const L_CUSTOM: u16 = 129;
    pub const L_RAINBOW: u16 = 130;
    pub const L_BRIGHT_LBL: u16 = 131;
    pub const L_BRIGHT: u16 = 132;
    pub const L_BRIGHT_VAL: u16 = 133;
    pub const L_SPEED_LBL: u16 = 134;
    pub const L_SPEED: u16 = 135;
    pub const L_SPEED_VAL: u16 = 136;
    pub const L_DIR_LBL: u16 = 137;
    pub const L_DIR: u16 = 138;
    pub const L_DIR_VAL: u16 = 139;
    pub const L_USES: u16 = 140;
    pub const L_APPLY: u16 = 141;
    pub const L_UNSENT: u16 = 142;
    pub const L_LED: u16 = 143;
    pub const C_LED: u16 = 149;

    pub const C_VIEW: u16 = 150;
    pub const C_HELP: u16 = 151;
    pub const C_BRUSH_LBL: u16 = 152;
    pub const C_PALETTE: u16 = 160; // 160..=169
    pub const C_CUSTOM: u16 = 170;
    pub const C_PAINT_ALL: u16 = 171;
    pub const C_ALL_OFF: u16 = 172;
    pub const C_LIT: u16 = 173;
    pub const C_BRIGHT_LBL: u16 = 174;
    pub const C_BRIGHT: u16 = 175;
    pub const C_BRIGHT_VAL: u16 = 176;
    pub const C_NOTE: u16 = 177;
    pub const C_APPLY: u16 = 178;
    pub const C_UNSENT: u16 = 179;

    pub const K_LAYER_LBL: u16 = 180;
    pub const K_NORMAL: u16 = 181;
    pub const K_FN: u16 = 182;
    pub const K_CHANGED: u16 = 183;
    pub const K_VIEW: u16 = 184;
    pub const K_SELECTED: u16 = 185;
    pub const K_BASIC_LBL: u16 = 186;
    pub const K_DEFAULT: u16 = 187;
    pub const K_DISABLED: u16 = 188;
    pub const K_KEY_LBL: u16 = 189;
    pub const K_CHOOSE: u16 = 190;
    pub const K_CTRL: u16 = 191;
    pub const K_SHIFT: u16 = 192;
    pub const K_ALT: u16 = 193;
    pub const K_WIN: u16 = 194;
    pub const K_MOUSE_LBL: u16 = 195;
    pub const K_MOUSE: u16 = 196;
    pub const K_MEDIA_LBL: u16 = 197;
    pub const K_MEDIA: u16 = 198;
    pub const K_MACRO_LBL: u16 = 199;
    pub const K_MACRO: u16 = 200;
    pub const K_APPLY: u16 = 201;
    pub const K_RESET: u16 = 202;
    pub const K_UNSENT: u16 = 203;

    pub const M_LIST_LBL: u16 = 210;
    pub const M_LIST: u16 = 211;
    pub const M_STEPS_LBL: u16 = 212;
    pub const M_STEPS: u16 = 213;
    pub const M_SAVE: u16 = 214;
    pub const M_DELETE: u16 = 215;
    pub const M_HELP: u16 = 216;
    pub const M_USE: u16 = 217;

    pub const S_WIN: u16 = 220;
    pub const S_ALT_TAB: u16 = 221;
    pub const S_ALT_F4: u16 = 222;
    pub const S_SLEEP_LBL: u16 = 223;
    pub const S_SLEEP: u16 = 224;
    pub const S_RESP_LBL: u16 = 225;
    pub const S_RESP: u16 = 226;
    pub const S_RESP_VAL: u16 = 227;
    pub const S_RESP_NOTE: u16 = 228;
    pub const S_APPLY: u16 = 229;
    pub const S_UNSENT: u16 = 230;

    pub const P_LIST_LBL: u16 = 240;
    pub const P_LIST: u16 = 241;
    pub const P_NAME_LBL: u16 = 242;
    pub const P_NAME: u16 = 243;
    pub const P_SAVE: u16 = 244;
    pub const P_SAVE_NOTE: u16 = 245;
    pub const P_LOAD: u16 = 246;
    pub const P_DELETE: u16 = 247;
    pub const P_LOAD_NOTE: u16 = 248;
    pub const P_FILES_LBL: u16 = 249;
    pub const P_IMPORT: u16 = 250;
    pub const P_EXPORT: u16 = 251;
    pub const P_FOLDER: u16 = 252;
    pub const P_FACTORY_LBL: u16 = 253;
    pub const P_FACTORY: u16 = 254;
    pub const P_FACTORY_NOTE: u16 = 255;
    pub const P_CONN_LBL: u16 = 256;
    pub const P_CONN: u16 = 257;
    pub const P_CHECK: u16 = 258;
    pub const P_DATA: u16 = 259;

    pub const A_NAME: u16 = 270;
    pub const A_ONLY: u16 = 271;
    pub const A_UNOFFICIAL: u16 = 272;
    pub const A_LICENSE_LBL: u16 = 273;
    pub const A_LICENSE: u16 = 274;
    pub const A_PRIVACY_LBL: u16 = 275;
    pub const A_PRIVACY: u16 = 276;
    pub const A_SOURCE_LBL: u16 = 277;
    pub const A_SOURCE: u16 = 278;
}

// ---------------------------------------------------------------- small helpers

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn send(h: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { SendMessageW(h, msg, Some(WPARAM(wp)), Some(LPARAM(lp))).0 }
}

fn post(h: isize, msg: u32, wp: usize, lp: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(h as *mut c_void)), msg, WPARAM(wp), LPARAM(lp));
    }
}

fn get_text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

fn set_text(h: HWND, s: &str) {
    if get_text(h) != s {
        let w = wide(s);
        unsafe {
            let _ = SetWindowTextW(h, PCWSTR(w.as_ptr()));
        }
    }
}

pub fn fatal(msg: &str) {
    let w = wide(msg);
    unsafe {
        MessageBoxW(None, PCWSTR(w.as_ptr()), w!("M87 Pro"), MB_OK | MB_ICONERROR);
    }
}

fn lang_file() -> std::path::PathBuf {
    state::data_dir().join("gui.txt")
}

fn load_lang() -> Lang {
    std::fs::read_to_string(lang_file())
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("language =").and_then(Lang::from_code)))
        .unwrap_or_else(system_lang)
}

fn save_lang(l: Lang) {
    let p = lang_file();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, format!("language = {}\n", l.code()));
}

// ---------------------------------------------------------------- controls

#[derive(Clone, Copy, PartialEq, Eq)]
enum Font {
    Normal,
    Bold,
    Mono,
    Title,
}

struct Ctl {
    hwnd: HWND,
    page: Option<Page>,
    rect: [i32; 4],
    text: Option<T>,
    font: Font,
    /// Owner-drawn button (drawn by `draw_item`).
    owner: bool,
}

struct Ui {
    main: HWND,
    lang: Lang,
    dpi: u32,
    font: HFONT,
    bold: HFONT,
    mono: HFONT,
    small: HFONT,
    ctls: Vec<Ctl>,
    by_id: HashMap<u16, usize>,
    /// Current items of each list or combo box, to avoid needless refills.
    items: HashMap<u16, Vec<String>>,
    /// Macro number whose text is in the steps box.
    macro_loaded: Option<u8>,
    battery_running: bool,
    /// Tooltip that names the color swatches.
    tip: HWND,
}

struct G {
    app: App,
    ui: Ui,
}

thread_local! {
    static G_CELL: RefCell<Option<G>> = const { RefCell::new(None) };
    static DRAG_LAST: Cell<Option<u8>> = const { Cell::new(None) };
    /// Page shown now, for drawing the navigation without the model.
    static SHOWN: Cell<u8> = const { Cell::new(0) };
    /// Text colors of the status line and the connection label.
    static LABEL_FG: Cell<(Rgb, Rgb)> = const { Cell::new((theme::TEXT, theme::TEXT)) };
}

/// One keyboard user at a time (sends and battery reads).
static HID_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with the model and controls, unless they are already in use
/// (a nested message) or not created yet.
fn with_g<R>(f: impl FnOnce(&mut G) -> R) -> Option<R> {
    G_CELL.with(|c| match c.try_borrow_mut() {
        Ok(mut b) => b.as_mut().map(f),
        Err(_) => None,
    })
}

impl Ui {
    fn h(&self, id: u16) -> HWND {
        self.by_id.get(&id).map(|i| self.ctls[*i].hwnd).unwrap_or_default()
    }

    fn s(&self, v: i32) -> i32 {
        v * self.dpi as i32 / 96
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, id: u16, page: Option<Page>, class: PCWSTR, text: Option<T>, style: u32, ex: u32, rect: [i32; 4], font: Font) {
        let hinst = unsafe { GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).ok() };
        let base = WS_CHILD | WINDOW_STYLE(style);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(ex),
                class,
                w!(""),
                base,
                0,
                0,
                10,
                10,
                Some(self.main),
                Some(HMENU(id as usize as *mut c_void)),
                hinst,
                None,
            )
            .unwrap_or_default()
        };
        self.by_id.insert(id, self.ctls.len());
        let owner = style & 0xF == BS_OWNERDRAW && unsafe { class.to_string() }.is_ok_and(|c| c == "BUTTON");
        self.ctls.push(Ctl { hwnd, page, rect, text, font, owner });
    }

    fn label(&mut self, id: u16, page: Option<Page>, text: Option<T>, rect: [i32; 4]) {
        self.add(id, page, w!("STATIC"), text, SS_LEFT | SS_NOPREFIX, 0, rect, Font::Normal);
    }

    fn button(&mut self, id: u16, page: Page, text: T, rect: [i32; 4]) {
        self.add(id, Some(page), w!("BUTTON"), Some(text), BS_OWNERDRAW | WS_TABSTOP.0, 0, rect, Font::Normal);
    }

    fn check(&mut self, id: u16, page: Page, text: T, rect: [i32; 4]) {
        self.add(id, Some(page), w!("BUTTON"), Some(text), BS_AUTOCHECKBOX | WS_TABSTOP.0, 0, rect, Font::Normal);
    }

    fn owner_button(&mut self, id: u16, page: Page, text: Option<T>, rect: [i32; 4]) {
        self.add(id, Some(page), w!("BUTTON"), text, BS_OWNERDRAW | WS_TABSTOP.0, 0, rect, Font::Normal);
    }

    fn combo(&mut self, id: u16, page: Option<Page>, rect: [i32; 4]) {
        self.add(id, page, w!("COMBOBOX"), None, CBS_DROPDOWNLIST | WS_TABSTOP.0 | WS_VSCROLL.0, 0, rect, Font::Normal);
    }

    fn list(&mut self, id: u16, page: Page, rect: [i32; 4]) {
        self.add(
            id,
            Some(page),
            w!("LISTBOX"),
            None,
            LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | WS_TABSTOP.0 | WS_VSCROLL.0,
            WS_EX_CLIENTEDGE.0,
            rect,
            Font::Normal,
        );
    }

    fn slider(&mut self, id: u16, page: Page, rect: [i32; 4], min: isize, max: isize) {
        self.add(id, Some(page), w!("msctls_trackbar32"), None, TBS_AUTOTICKS | WS_TABSTOP.0, 0, rect, Font::Normal);
        let h = self.h(id);
        send(h, TBM_SETRANGEMIN, 0, min);
        send(h, TBM_SETRANGEMAX, 1, max);
    }

    /// Create every control. Positions are set by `layout`.
    fn build(&mut self) {
        use id::*;
        use Page::*;
        // Fonts first: list boxes ask for their row height while created.
        self.make_fonts();
        // Navigation column.
        for (i, p) in Page::ALL.iter().enumerate() {
            let y = if *p == About { 466 } else { 16 + i as i32 * 40 };
            self.add(NAV + i as u16, None, w!("BUTTON"), Some(p.title()), BS_OWNERDRAW | WS_TABSTOP.0, 0, [8, y, 152, 36], Font::Normal);
        }
        self.label(LANG_LBL, None, Some(T::Language), [16, 516, 136, 20]);
        self.combo(LANG, None, [12, 538, 144, 200]);
        self.label(CONN, None, None, [16, 580, 140, 92]);
        // Page title above the card, status line below it.
        self.add(TITLE, None, w!("STATIC"), None, SS_LEFT | SS_NOPREFIX, 0, [184, 16, 700, 40], Font::Title);
        self.label(STATUS, None, None, [176, 622, 956, 50]);

        // 1 Lighting
        self.label(L_EFFECT_LBL, Some(Lighting), Some(T::Effect), [24, 78, 200, 20]);
        self.list(L_EFFECTS, Lighting, [24, 100, 200, 470]);
        self.label(L_COLOR_LBL, Some(Lighting), Some(T::Color), [250, 102, 110, 20]);
        for i in 0..9 {
            self.owner_button(L_PALETTE + i, Lighting, Some(PALETTE[i as usize].0), [370 + i as i32 * 40, 90, 34, 34]);
        }
        self.owner_button(L_CUSTOM, Lighting, None, [370, 132, 200, 30]);
        self.check(L_RAINBOW, Lighting, T::Rainbow, [370, 168, 400, 24]);
        for (lbl, tb, val, y, max) in [
            (L_BRIGHT_LBL, L_BRIGHT, L_BRIGHT_VAL, 214, MAX_LEVEL as isize),
            (L_SPEED_LBL, L_SPEED, L_SPEED_VAL, 254, MAX_LEVEL as isize),
            (L_DIR_LBL, L_DIR, L_DIR_VAL, 294, 3),
        ] {
            let t = match lbl {
                L_BRIGHT_LBL => T::Brightness,
                L_SPEED_LBL => T::Speed,
                _ => T::Direction,
            };
            self.label(lbl, Some(Lighting), Some(t), [250, y, 110, 20]);
            self.slider(tb, Lighting, [366, y - 6, 300, 32], 0, max);
            self.label(val, Some(Lighting), None, [676, y, 60, 20]);
        }
        self.label(L_USES, Some(Lighting), None, [250, 344, 690, 40]);
        self.label(L_LED, Some(Lighting), Some(T::LedNote), [250, 470, 700, 60]);
        self.button(L_APPLY, Lighting, T::Apply, [250, 560, 200, 34]);
        self.label(L_UNSENT, Some(Lighting), None, [464, 568, 480, 20]);

        // 2 Key colors
        self.add(C_VIEW, Some(KeyColors), w!("M87KeyboardView"), None, 0, 0, [24, 76, 932, 250], Font::Normal);
        self.label(C_HELP, Some(KeyColors), Some(T::KeyColorsHelp), [24, 332, 932, 20]);
        self.label(C_BRUSH_LBL, Some(KeyColors), Some(T::BrushColor), [24, 366, 110, 20]);
        for i in 0..10 {
            self.owner_button(C_PALETTE + i, KeyColors, Some(PALETTE[i as usize].0), [140 + i as i32 * 40, 356, 34, 34]);
        }
        self.owner_button(C_CUSTOM, KeyColors, None, [560, 358, 200, 30]);
        self.button(C_PAINT_ALL, KeyColors, T::PaintAll, [140, 400, 170, 30]);
        self.button(C_ALL_OFF, KeyColors, T::AllOff, [320, 400, 170, 30]);
        self.label(C_LIT, Some(KeyColors), None, [504, 406, 300, 20]);
        self.label(C_BRIGHT_LBL, Some(KeyColors), Some(T::Brightness), [24, 452, 110, 20]);
        self.slider(C_BRIGHT, KeyColors, [136, 446, 300, 32], 0, MAX_LEVEL as isize);
        self.label(C_BRIGHT_VAL, Some(KeyColors), None, [446, 452, 60, 20]);
        self.label(C_NOTE, Some(KeyColors), Some(T::KeyColorsNote), [24, 494, 932, 20]);
        self.label(C_LED, Some(KeyColors), Some(T::LedNote), [24, 516, 932, 40]);
        self.button(C_APPLY, KeyColors, T::Apply, [24, 560, 200, 34]);
        self.label(C_UNSENT, Some(KeyColors), None, [238, 568, 480, 20]);

        // 3 Key mapping
        self.label(K_LAYER_LBL, Some(Keys), Some(T::Layer), [24, 80, 80, 20]);
        self.add(K_NORMAL, Some(Keys), w!("BUTTON"), Some(T::LayerNormal), BS_AUTORADIOBUTTON | WS_GROUP.0 | WS_TABSTOP.0, 0, [110, 78, 120, 24], Font::Normal);
        self.add(K_FN, Some(Keys), w!("BUTTON"), Some(T::LayerFn), BS_AUTORADIOBUTTON, 0, [236, 78, 230, 24], Font::Normal);
        self.label(K_CHANGED, Some(Keys), Some(T::ChangedYellow), [480, 80, 476, 20]);
        self.add(K_VIEW, Some(Keys), w!("M87KeyboardView"), None, 0, 0, [24, 106, 932, 272], Font::Normal);
        self.label(K_SELECTED, Some(Keys), None, [24, 386, 932, 22]);
        self.label(K_BASIC_LBL, Some(Keys), Some(T::Basic), [24, 424, 90, 20]);
        self.button(K_DEFAULT, Keys, T::NormalFunction, [120, 418, 160, 30]);
        self.button(K_DISABLED, Keys, T::DoNothing, [288, 418, 140, 30]);
        self.label(K_KEY_LBL, Some(Keys), Some(T::KeyRow), [24, 466, 90, 20]);
        self.button(K_CHOOSE, Keys, T::ChooseKey, [120, 460, 220, 30]);
        for (i, (cid, name)) in [(K_CTRL, "Ctrl"), (K_SHIFT, "Shift"), (K_ALT, "Alt"), (K_WIN, "Win")].into_iter().enumerate() {
            self.add(cid, Some(Keys), w!("BUTTON"), None, BS_AUTOCHECKBOX | WS_TABSTOP.0, 0, [120 + i as i32 * 76, 498, 72, 24], Font::Normal);
            set_text(self.h(cid), name);
        }
        // Second column: the other kinds of action.
        self.label(K_MOUSE_LBL, Some(Keys), Some(T::Mouse), [520, 424, 100, 20]);
        self.combo(K_MOUSE, Some(Keys), [626, 420, 300, 300]);
        self.label(K_MEDIA_LBL, Some(Keys), Some(T::Media), [520, 466, 100, 20]);
        self.combo(K_MEDIA, Some(Keys), [626, 462, 300, 400]);
        self.label(K_MACRO_LBL, Some(Keys), Some(T::Macro), [520, 508, 100, 20]);
        self.combo(K_MACRO, Some(Keys), [626, 504, 300, 300]);
        self.button(K_APPLY, Keys, T::Apply, [24, 560, 200, 34]);
        self.button(K_RESET, Keys, T::ResetAll, [234, 560, 240, 34]);
        self.label(K_UNSENT, Some(Keys), None, [488, 568, 470, 20]);

        // 4 Macros
        self.label(M_LIST_LBL, Some(Macros), Some(T::MacroList), [24, 78, 420, 20]);
        self.list(M_LIST, Macros, [24, 100, 420, 480]);
        self.label(M_STEPS_LBL, Some(Macros), None, [464, 100, 490, 20]);
        self.add(
            M_STEPS,
            Some(Macros),
            w!("EDIT"),
            None,
            ES_MULTILINE | ES_AUTOVSCROLL | WS_VSCROLL.0 | WS_TABSTOP.0,
            WS_EX_CLIENTEDGE.0,
            [464, 122, 490, 90],
            Font::Mono,
        );
        self.button(M_SAVE, Macros, T::SaveSend, [464, 222, 180, 30]);
        self.button(M_DELETE, Macros, T::Delete, [654, 222, 120, 30]);
        self.add(M_HELP, Some(Macros), w!("STATIC"), Some(T::MacroHelp), SS_LEFT | SS_NOPREFIX, 0, [464, 270, 490, 150], Font::Mono);
        self.label(M_USE, Some(Macros), Some(T::MacroUse), [464, 430, 490, 40]);

        // 5 Settings
        self.check(S_WIN, Settings, T::BlockWin, [24, 84, 500, 24]);
        self.check(S_ALT_TAB, Settings, T::BlockAltTab, [24, 114, 500, 24]);
        self.check(S_ALT_F4, Settings, T::BlockAltF4, [24, 144, 500, 24]);
        self.label(S_SLEEP_LBL, Some(Settings), Some(T::Sleep), [24, 196, 260, 40]);
        self.combo(S_SLEEP, Some(Settings), [290, 192, 200, 200]);
        self.label(S_RESP_LBL, Some(Settings), Some(T::Response), [24, 246, 260, 20]);
        self.slider(S_RESP, Settings, [286, 240, 250, 32], 1, 5);
        self.label(S_RESP_VAL, Some(Settings), None, [546, 246, 60, 20]);
        self.label(S_RESP_NOTE, Some(Settings), Some(T::ResponseNote), [290, 278, 660, 20]);
        self.button(S_APPLY, Settings, T::Apply, [24, 560, 200, 34]);
        self.label(S_UNSENT, Some(Settings), None, [238, 568, 480, 20]);

        // 6 Profiles
        self.add(P_LIST_LBL, Some(Profiles), w!("STATIC"), Some(T::SavedProfiles), SS_LEFT | SS_NOPREFIX, 0, [24, 78, 300, 20], Font::Bold);
        self.list(P_LIST, Profiles, [24, 100, 300, 300]);
        self.label(P_NAME_LBL, Some(Profiles), Some(T::Name), [344, 104, 80, 20]);
        self.add(P_NAME, Some(Profiles), w!("EDIT"), None, ES_AUTOHSCROLL | WS_TABSTOP.0, WS_EX_CLIENTEDGE.0, [430, 100, 300, 26], Font::Normal);
        self.button(P_SAVE, Profiles, T::SaveCurrent, [740, 99, 214, 28]);
        self.label(P_SAVE_NOTE, Some(Profiles), Some(T::SaveNote), [430, 132, 524, 36]);
        self.button(P_LOAD, Profiles, T::Load, [344, 180, 200, 30]);
        self.button(P_DELETE, Profiles, T::Delete, [554, 180, 120, 30]);
        self.label(P_LOAD_NOTE, Some(Profiles), Some(T::LoadNote), [344, 216, 610, 20]);
        self.add(P_FILES_LBL, Some(Profiles), w!("STATIC"), Some(T::ProfileFiles), SS_LEFT | SS_NOPREFIX, 0, [344, 256, 300, 20], Font::Bold);
        self.button(P_IMPORT, Profiles, T::Import, [344, 280, 150, 30]);
        self.button(P_EXPORT, Profiles, T::Export, [504, 280, 150, 30]);
        self.button(P_FOLDER, Profiles, T::OpenFolder, [664, 280, 220, 30]);
        self.add(P_FACTORY_LBL, Some(Profiles), w!("STATIC"), Some(T::FactoryTitle), SS_LEFT | SS_NOPREFIX, 0, [24, 420, 300, 20], Font::Bold);
        self.button(P_FACTORY, Profiles, T::FactoryReset, [24, 444, 200, 30]);
        self.label(P_FACTORY_NOTE, Some(Profiles), Some(T::FactoryNote), [234, 446, 720, 40]);
        self.add(P_CONN_LBL, Some(Profiles), w!("STATIC"), Some(T::Connection), SS_LEFT | SS_NOPREFIX, 0, [24, 500, 300, 20], Font::Bold);
        self.label(P_CONN, Some(Profiles), None, [24, 524, 600, 20]);
        self.button(P_CHECK, Profiles, T::CheckAgain, [630, 518, 160, 30]);
        self.label(P_DATA, Some(Profiles), None, [24, 556, 930, 20]);

        // 7 About
        self.add(A_NAME, Some(About), w!("STATIC"), None, SS_LEFT | SS_NOPREFIX, 0, [24, 84, 930, 22], Font::Bold);
        self.label(A_ONLY, Some(About), Some(T::AboutOnly), [24, 116, 930, 40]);
        self.label(A_UNOFFICIAL, Some(About), Some(T::AboutUnofficial), [24, 162, 930, 40]);
        self.add(A_LICENSE_LBL, Some(About), w!("STATIC"), Some(T::AboutLicenseTitle), SS_LEFT | SS_NOPREFIX, 0, [24, 220, 300, 20], Font::Bold);
        self.label(A_LICENSE, Some(About), Some(T::AboutLicense), [24, 244, 930, 40]);
        self.add(A_PRIVACY_LBL, Some(About), w!("STATIC"), Some(T::AboutPrivacyTitle), SS_LEFT | SS_NOPREFIX, 0, [24, 302, 300, 20], Font::Bold);
        self.label(A_PRIVACY, Some(About), Some(T::AboutPrivacy), [24, 326, 930, 40]);
        self.add(A_SOURCE_LBL, Some(About), w!("STATIC"), Some(T::AboutSourceTitle), SS_LEFT | SS_NOPREFIX, 0, [24, 384, 300, 20], Font::Bold);
        self.label(A_SOURCE, Some(About), None, [24, 408, 930, 20]);

        // Swatches show only a color; the tooltip gives its name.
        unsafe {
            let hinst = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).ok();
            self.tip = CreateWindowExW(
                WS_EX_TOPMOST,
                w!("tooltips_class32"),
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(self.main),
                None,
                hinst,
                None,
            )
            .unwrap_or_default();
        }
        for cid in (L_PALETTE..L_PALETTE + 9).chain(C_PALETTE..C_PALETTE + 10) {
            let ti = TTTOOLINFOW {
                cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                uFlags: TOOLTIP_FLAGS(TTF_IDISHWND | TTF_SUBCLASS),
                hwnd: self.main,
                uId: self.h(cid).0 as usize,
                // LPSTR_TEXTCALLBACKW: ask for the text when shown.
                lpszText: PWSTR(usize::MAX as *mut u16),
                ..Default::default()
            };
            send(self.tip, TTM_ADDTOOLW, 0, &ti as *const _ as isize);
        }

    }

    /// Fonts for the current DPI.
    fn make_fonts(&mut self) {
        unsafe {
            let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let _ = SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                ncm.cbSize,
                Some(&mut ncm as *mut _ as *mut c_void),
                0,
                self.dpi,
            );
            for f in [self.font, self.bold, self.mono, self.small] {
                if !f.is_invalid() {
                    let _ = DeleteObject(f.into());
                }
            }
            let face = |lf: &mut LOGFONTW, name: &str| {
                lf.lfFaceName = [0; 32];
                for (i, c) in name.encode_utf16().take(31).enumerate() {
                    lf.lfFaceName[i] = c;
                }
            };
            let mut base = ncm.lfMessageFont;
            // Korean Windows uses a font that shows "\" as a won sign;
            // English text gets Segoe UI (Korean letters still fall back).
            if self.lang == Lang::En {
                face(&mut base, "Segoe UI");
            }
            self.font = CreateFontIndirectW(&base);
            theme::set_fonts(self.dpi, &base, self.font);
            let mut b = base;
            b.lfWeight = 700;
            self.bold = CreateFontIndirectW(&b);
            let mut s = base;
            s.lfHeight = base.lfHeight * 85 / 100;
            face(&mut s, "Segoe UI");
            self.small = CreateFontIndirectW(&s);
            let mut m = base;
            face(&mut m, "Consolas");
            self.mono = CreateFontIndirectW(&m);
        }
    }

    fn layout(&mut self) {
        self.make_fonts();
        for c in &self.ctls {
            let [x, y, w, h] = c.rect;
            let x = if c.page.is_some() { x + CONTENT_DX } else { x };
            let font = match c.font {
                Font::Normal => self.font,
                Font::Bold => self.bold,
                Font::Mono => self.mono,
                Font::Title => theme::title_font(),
            };
            unsafe {
                let _ = SetWindowPos(c.hwnd, None, self.s(x), self.s(y), self.s(w), self.s(h), SWP_NOZORDER | SWP_NOACTIVATE);
            }
            send(c.hwnd, WM_SETFONT, font.0 as usize, 1);
        }
        for cid in [id::L_EFFECTS, id::M_LIST, id::P_LIST] {
            send(self.h(cid), LB_SETITEMHEIGHT, 0, self.s(LIST_ROW) as isize);
        }
    }

    /// Texts that depend only on the language.
    fn apply_language(&mut self, lang: Lang) {
        if self.lang != lang {
            self.lang = lang;
            self.layout();
        }
        let title = wide(tr(lang, T::AppTitle));
        unsafe {
            let _ = SetWindowTextW(self.main, PCWSTR(title.as_ptr()));
        }
        for c in &self.ctls {
            if let Some(t) = c.text {
                set_text(c.hwnd, tr(lang, t));
            }
        }
        // Lists whose items are translated.
        self.items.remove(&id::L_EFFECTS);
        self.items.remove(&id::K_MOUSE);
        self.items.remove(&id::K_MEDIA);
        self.items.remove(&id::K_MACRO);
        self.items.remove(&id::S_SLEEP);
        self.items.remove(&id::M_LIST);
        unsafe {
            let _ = InvalidateRect(Some(self.main), None, true);
        }
    }

    fn show_page(&self, page: Page) {
        for c in &self.ctls {
            if let Some(p) = c.page {
                unsafe {
                    let _ = ShowWindow(c.hwnd, if p == page { SW_SHOW } else { SW_HIDE });
                }
            } else {
                unsafe {
                    let _ = ShowWindow(c.hwnd, SW_SHOW);
                }
            }
        }
        SHOWN.with(|s| s.set(page as u8));
        set_text(self.h(id::TITLE), tr(self.lang, page.title()));
        for i in 0..Page::ALL.len() as u16 {
            self.invalidate(id::NAV + i);
        }
    }

    fn set_items(&mut self, cid: u16, items: Vec<String>, combo: bool) {
        if self.items.get(&cid) == Some(&items) {
            return;
        }
        let h = self.h(cid);
        let (reset, add) = if combo { (CB_RESETCONTENT, CB_ADDSTRING) } else { (LB_RESETCONTENT, LB_ADDSTRING) };
        send(h, reset, 0, 0);
        for it in &items {
            let w = wide(it);
            send(h, add, 0, w.as_ptr() as isize);
        }
        self.items.insert(cid, items);
    }

    fn set_sel(&self, cid: u16, sel: Option<usize>, combo: bool) {
        let h = self.h(cid);
        // Leave an open drop-down list alone; the user is choosing.
        if combo && send(h, CB_GETDROPPEDSTATE, 0, 0) != 0 {
            return;
        }
        let want = sel.map(|s| s as isize).unwrap_or(-1);
        let get = if combo { CB_GETCURSEL } else { LB_GETCURSEL };
        if send(h, get, 0, 0) != want {
            send(h, if combo { CB_SETCURSEL } else { LB_SETCURSEL }, want as usize, 0);
        }
    }

    fn sel(&self, cid: u16, combo: bool) -> Option<usize> {
        let v = send(self.h(cid), if combo { CB_GETCURSEL } else { LB_GETCURSEL }, 0, 0);
        (v >= 0).then_some(v as usize)
    }

    fn set_check(&self, cid: u16, on: bool) {
        send(self.h(cid), BM_SETCHECK, on as usize, 0);
    }

    fn checked(&self, cid: u16) -> bool {
        send(self.h(cid), BM_GETCHECK, 0, 0) == 1
    }

    fn enable(&self, cid: u16, on: bool) {
        unsafe {
            let h = self.h(cid);
            if IsWindowEnabled(h).as_bool() != on {
                let _ = EnableWindow(h, on);
            }
        }
    }

    fn set_slider(&self, cid: u16, v: u8) {
        let h = self.h(cid);
        if send(h, TBM_GETPOS, 0, 0) != v as isize {
            send(h, TBM_SETPOS, 1, v as isize);
        }
    }

    fn invalidate(&self, cid: u16) {
        unsafe {
            let _ = InvalidateRect(Some(self.h(cid)), None, false);
        }
    }
}

// ---------------------------------------------------------------- sync model -> controls

fn sync(g: &mut G) {
    sync_controls(g);
    // Owner-drawn buttons may have been painted while the model was in use
    // (a button repaints at once when its text changes); draw them again.
    let page = g.app.page;
    for c in g.ui.ctls.iter().filter(|c| c.owner && c.page == Some(page)) {
        unsafe {
            let _ = InvalidateRect(Some(c.hwnd), None, false);
        }
    }
}

fn sync_controls(g: &mut G) {
    let G { app, ui } = g;
    let lang = app.lang;
    let t = |x: T| tr(lang, x);
    use id::*;

    // Top bar and status.
    let conn = match (app.link, app.battery) {
        (Some(Link::Dongle), Some(b)) => format!("{}\n{} {b}%", t(T::ConnDongle), t(T::Battery)),
        (Some(Link::Dongle), None) => t(T::ConnDongle).to_string(),
        (Some(Link::Wired), _) => format!("{}\n{} {}", t(T::ConnWired), t(T::Battery), t(T::Charging)),
        (None, _) => t(T::NotConnected).to_string(),
    };
    set_text(ui.h(CONN), &conn);
    set_text(ui.h(TITLE), t(app.page.title()));
    let status_fg = match app.status.0 {
        Tone::Err => theme::ERR_TEXT,
        Tone::Ok => theme::OK_TEXT,
        Tone::Info => theme::TEXT,
    };
    let conn_fg = if app.link.is_some() { theme::OK_TEXT } else { theme::ERR_TEXT };
    LABEL_FG.with(|c| c.set((status_fg, conn_fg)));
    set_text(ui.h(STATUS), &app.status.1);
    ui.invalidate(STATUS);
    ui.invalidate(CONN);
    ui.set_items(LANG, Lang::ALL.iter().map(|l| l.name().to_string()).collect(), true);
    ui.set_sel(LANG, Lang::ALL.iter().position(|l| *l == lang), true);
    let unsent = if app.unsent() { t(T::Unsent) } else { "" };

    match app.page {
        Page::Lighting => {
            ui.set_items(L_EFFECTS, Effect::ALL.iter().map(|e| effect_name(lang, *e).to_string()).collect(), false);
            ui.set_sel(L_EFFECTS, Some(app.lighting.effect.id() as usize), false);
            let c = app.lighting.effect.controls();
            for i in 0..9 {
                ui.enable(L_PALETTE + i, c.color);
                ui.invalidate(L_PALETTE + i);
            }
            ui.enable(L_CUSTOM, c.color);
            ui.invalidate(L_CUSTOM);
            ui.enable(L_RAINBOW, c.color);
            ui.set_check(L_RAINBOW, app.lighting.rainbow);
            for (tb, val, v, max, on) in [
                (L_BRIGHT, L_BRIGHT_VAL, app.lighting.brightness, MAX_LEVEL, c.brightness),
                (L_SPEED, L_SPEED_VAL, app.lighting.speed, MAX_LEVEL, c.speed),
                (L_DIR, L_DIR_VAL, app.lighting.direction, 3, c.direction),
            ] {
                ui.set_slider(tb, v);
                ui.enable(tb, on);
                set_text(ui.h(val), &format!("{v} / {max}"));
            }
            let mut uses = Vec::new();
            if c.color {
                uses.push(t(T::Color));
            }
            if c.brightness {
                uses.push(t(T::Brightness));
            }
            if c.speed {
                uses.push(t(T::Speed));
            }
            if c.direction {
                uses.push(t(T::Direction));
            }
            let text = if uses.is_empty() { t(T::UsesNone).to_string() } else { format!("{} {}", t(T::Uses), uses.join(", ")) };
            set_text(ui.h(L_USES), &text);
            set_text(ui.h(L_UNSENT), unsent);
        }
        Page::KeyColors => {
            ui.invalidate(C_VIEW);
            for i in 0..10 {
                ui.invalidate(C_PALETTE + i);
            }
            ui.invalidate(C_CUSTOM);
            set_text(ui.h(C_LIT), &format!("{} / {} {}", app.lit_count(), m87_proto::layout::KEYS.len(), t(T::KeysLit)));
            ui.set_slider(C_BRIGHT, app.key_brightness);
            set_text(ui.h(C_BRIGHT_VAL), &format!("{} / {MAX_LEVEL}", app.key_brightness));
            set_text(ui.h(C_UNSENT), unsent);
        }
        Page::Keys => {
            ui.set_check(K_NORMAL, app.layer == Layer::Base);
            ui.set_check(K_FN, app.layer == Layer::Fn);
            ui.invalidate(K_VIEW);
            let current = app.current_action();
            let editable = app.map_key.is_some_and(|k| k.id != "Fn");
            let selected = match app.map_key {
                None => t(T::ClickKey).to_string(),
                Some(k) if k.id == "Fn" => format!("{} Fn — {}", t(T::Selected), t(T::FnFixed)),
                Some(k) => format!(
                    "{} {}  ({})   {} {}",
                    t(T::Selected),
                    k.id,
                    t(if app.layer == Layer::Base { T::LayerNormal } else { T::LayerFn }),
                    t(T::NowDoes),
                    app.action_text(current.unwrap_or(KeyAction::Default))
                ),
            };
            set_text(ui.h(K_SELECTED), &selected);
            for c in [K_DEFAULT, K_DISABLED, K_CHOOSE, K_MOUSE, K_MEDIA, K_MACRO] {
                ui.enable(c, editable);
            }
            let (mods, usage) = match current {
                Some(KeyAction::Key { modifiers, usage }) => (modifiers, usage),
                _ => (0, 0),
            };
            let real_key = usage != 0 && !(0xE0..=0xE7).contains(&usage);
            for (c, bit) in [(K_CTRL, modifier::LCTRL), (K_SHIFT, modifier::LSHIFT), (K_ALT, modifier::LALT), (K_WIN, modifier::LWIN)] {
                ui.enable(c, editable && real_key);
                ui.set_check(c, mods & bit != 0);
            }
            let choose = if app.choosing_key {
                t(T::ChooseKeyHint).split('.').next().unwrap_or("").to_string()
            } else if usage != 0 {
                format!("{} {}", t(T::Sends), app.action_text(KeyAction::Key { modifiers: mods, usage }))
            } else {
                t(T::ChooseKey).to_string()
            };
            set_text(ui.h(K_CHOOSE), &choose);

            let mut mouse = vec![t(T::ChooseOne).to_string()];
            mouse.extend(Mouse::ALL.iter().map(|m| mouse_name(lang, *m).to_string()));
            ui.set_items(K_MOUSE, mouse, true);
            ui.set_sel(K_MOUSE, Some(match current {
                Some(KeyAction::Mouse(m)) => Mouse::ALL.iter().position(|x| *x == m).map_or(0, |i| i + 1),
                _ => 0,
            }), true);
            let mut media = vec![t(T::ChooseOne).to_string()];
            media.extend(MEDIA.iter().map(|(n, _)| media_name(lang, n).to_string()));
            ui.set_items(K_MEDIA, media, true);
            ui.set_sel(K_MEDIA, Some(match current {
                Some(KeyAction::Media(u)) => MEDIA.iter().position(|(_, v)| *v == u).map_or(0, |i| i + 1),
                _ => 0,
            }), true);
            let numbers: Vec<u8> = app.state.macros.keys().copied().collect();
            let mut macros = vec![t(if numbers.is_empty() { T::NoMacros } else { T::ChooseOne }).to_string()];
            macros.extend(numbers.iter().map(|n| format!("{} {n}", t(T::Macro))));
            ui.set_items(K_MACRO, macros, true);
            ui.set_sel(K_MACRO, Some(match current {
                Some(KeyAction::Macro { slot, .. }) => numbers.iter().position(|n| *n == slot + 1).map_or(0, |i| i + 1),
                _ => 0,
            }), true);
            ui.enable(K_MACRO, editable && !numbers.is_empty());
            set_text(ui.h(K_UNSENT), unsent);
        }
        Page::Macros => {
            let items: Vec<String> = (1..=100u8)
                .map(|n| {
                    let text = app.macro_text(n);
                    format!("{n:>3}   {}", if text.is_empty() { t(T::Empty).to_string() } else { text })
                })
                .collect();
            ui.set_items(M_LIST, items, false);
            ui.set_sel(M_LIST, Some(app.macro_sel as usize - 1), false);
            set_text(ui.h(M_STEPS_LBL), &format!("{} {}", t(T::StepsOf), app.macro_sel));
            if ui.macro_loaded != Some(app.macro_sel) {
                set_text(ui.h(M_STEPS), &app.macro_text(app.macro_sel));
                ui.macro_loaded = Some(app.macro_sel);
            }
            ui.enable(M_DELETE, app.state.macros.contains_key(&app.macro_sel));
        }
        Page::Settings => {
            let s = app.state.settings;
            ui.set_check(S_WIN, s.disable_win);
            ui.set_check(S_ALT_TAB, s.disable_alt_tab);
            ui.set_check(S_ALT_F4, s.disable_alt_f4);
            ui.set_items(S_SLEEP, [T::Never, T::Min1, T::Min5, T::Min30].iter().map(|x| t(*x).to_string()).collect(), true);
            ui.set_sel(S_SLEEP, SLEEP_CHOICES.iter().position(|(_, v)| *v == s.sleep), true);
            ui.set_slider(S_RESP, s.response);
            set_text(ui.h(S_RESP_VAL), &format!("{} / 5", s.response));
            set_text(ui.h(S_UNSENT), unsent);
        }
        Page::Profiles => {
            ui.set_items(P_LIST, app.profiles.clone(), false);
            ui.set_sel(P_LIST, app.profile_sel, false);
            let has = app.profile_sel.is_some();
            ui.enable(P_LOAD, has);
            ui.enable(P_DELETE, has);
            set_text(ui.h(P_CONN), &conn.replace('\n', "     "));
            set_text(ui.h(P_DATA), &format!("{} {}", t(T::DataFolder), state::data_dir().display()));
        }
        Page::About => {
            set_text(ui.h(A_NAME), &format!("m87-gui {}  -  {}", env!("CARGO_PKG_VERSION"), t(T::AboutName)));
            set_text(ui.h(A_SOURCE), "https://github.com/rity73/m87pro-tool");
        }
    }
}

// ---------------------------------------------------------------- background work

fn spawn_send(hwnd: isize, packets: Packets) {
    std::thread::spawn(move || {
        let _lock = HID_LOCK.lock();
        let result: Result<Link, HidError> = Keyboard::open().and_then(|kb| {
            let link = kb.link();
            kb.run(packets.for_link(link)).map(|_| link)
        });
        post(hwnd, WM_APP_DONE, 0, Box::into_raw(Box::new(result)) as isize);
    });
}

fn spawn_battery(hwnd: isize) {
    std::thread::spawn(move || {
        let _lock = HID_LOCK.lock();
        let v = Keyboard::open().and_then(|kb| kb.battery()).map(|b| b as usize).unwrap_or(0xFFFF);
        post(hwnd, WM_APP_BATTERY, v, 0);
    });
}

/// Find the keyboard; read the battery when it is wireless and unknown.
fn scan(g: &mut G, force_battery: bool) {
    let link = list_interfaces().iter().find(|i| i.role == Role::Config).map(|i| i.link);
    if link != g.app.link {
        g.app.battery = None;
    }
    g.app.link = link;
    let want = link == Some(Link::Dongle) && (force_battery || g.app.battery.is_none());
    if want && !g.ui.battery_running && g.app.busy.is_none() {
        g.ui.battery_running = true;
        spawn_battery(g.ui.main.0 as isize);
    }
}

// ---------------------------------------------------------------- input handling

/// Work that must run without the model borrowed (modal windows).
enum Later {
    PickColor { brush: bool, start: Rgb },
    ConfirmLoad(String),
    ConfirmDelete(String),
    ConfirmFactory,
    Import,
    Export,
}

fn start_job(g: &mut G, job: Job) {
    if let Some(p) = g.app.start(job) {
        spawn_send(g.ui.main.0 as isize, p);
    }
}

fn on_command(g: &mut G, cid: u16, code: u32) -> Option<Later> {
    use id::*;
    let app = &mut g.app;
    let ui = &mut g.ui;
    match cid {
        IDCANCEL_CMD => app.cancel(),
        c if (NAV..NAV + Page::ALL.len() as u16).contains(&c) => {
            app.page = Page::ALL[(c - NAV) as usize];
            app.choosing_key = false;
            if app.page == Page::Profiles {
                let sel = app.selected_profile();
                app.refresh_profiles(sel.as_deref());
            }
            ui.show_page(app.page);
        }
        IDOK_CMD => {
            let focus = unsafe { GetFocus() };
            if focus == ui.h(P_NAME) {
                return on_command(g, P_SAVE, BN_CLICKED);
            }
            if focus == ui.h(M_STEPS) {
                return on_command(g, M_SAVE, BN_CLICKED);
            }
        }
        LANG if code == CBN_SELCHANGE => {
            if let Some(l) = ui.sel(LANG, true).and_then(|i| Lang::ALL.get(i)) {
                app.lang = *l;
                save_lang(*l);
                ui.apply_language(*l);
                app.status = (Tone::Info, tr(*l, T::Ready).into());
            }
        }
        L_EFFECTS if code == LBN_SELCHANGE => {
            if let Some(e) = ui.sel(L_EFFECTS, false).and_then(|i| Effect::ALL.get(i)) {
                app.lighting.effect = *e;
            }
        }
        c if (L_PALETTE..L_PALETTE + 9).contains(&c) => app.set_effect_color(PALETTE[(c - L_PALETTE) as usize].1),
        L_CUSTOM => return Some(Later::PickColor { brush: false, start: app.lighting.color }),
        L_RAINBOW => app.lighting.rainbow = ui.checked(L_RAINBOW),
        L_APPLY | C_APPLY | K_APPLY | S_APPLY => {
            if let Some(job) = app.page_job() {
                start_job(g, job);
            }
        }
        c if (C_PALETTE..C_PALETTE + 10).contains(&c) => app.brush = PALETTE[(c - C_PALETTE) as usize].1,
        C_CUSTOM => return Some(Later::PickColor { brush: true, start: app.brush }),
        C_PAINT_ALL => app.paint_all(app.brush),
        C_ALL_OFF => app.paint_all(Rgb::default()),
        K_NORMAL => app.layer = Layer::Base,
        K_FN => app.layer = Layer::Fn,
        K_DEFAULT => app.set_action(KeyAction::Default),
        K_DISABLED => app.set_action(KeyAction::Disabled),
        K_CHOOSE => app.choose_key(),
        K_CTRL => app.toggle_modifier(modifier::LCTRL),
        K_SHIFT => app.toggle_modifier(modifier::LSHIFT),
        K_ALT => app.toggle_modifier(modifier::LALT),
        K_WIN => app.toggle_modifier(modifier::LWIN),
        K_MOUSE if code == CBN_SELCHANGE => {
            if let Some(m) = ui.sel(K_MOUSE, true).filter(|i| *i > 0).and_then(|i| Mouse::ALL.get(i - 1)) {
                app.set_action(KeyAction::Mouse(*m));
            }
        }
        K_MEDIA if code == CBN_SELCHANGE => {
            if let Some((_, u)) = ui.sel(K_MEDIA, true).filter(|i| *i > 0).and_then(|i| MEDIA.get(i - 1)) {
                app.set_action(KeyAction::Media(*u));
            }
        }
        K_MACRO if code == CBN_SELCHANGE => {
            let numbers: Vec<u8> = app.state.macros.keys().copied().collect();
            if let Some(n) = ui.sel(K_MACRO, true).filter(|i| *i > 0).and_then(|i| numbers.get(i - 1)) {
                app.set_action(KeyAction::Macro { slot: n - 1, mode: 0, count: 1 });
            }
        }
        K_RESET => start_job(g, Job::ResetKeys),
        M_LIST if code == LBN_SELCHANGE => {
            if let Some(i) = ui.sel(M_LIST, false) {
                app.macro_sel = i as u8 + 1;
            }
        }
        M_SAVE => {
            let text = get_text(ui.h(M_STEPS));
            if app.save_macro(&text) {
                ui.macro_loaded = None;
                start_job(g, Job::Macros);
            }
        }
        M_DELETE => {
            if app.delete_macro() {
                ui.macro_loaded = None;
                start_job(g, Job::Macros);
            }
        }
        S_WIN => app.state.settings.disable_win = ui.checked(S_WIN),
        S_ALT_TAB => app.state.settings.disable_alt_tab = ui.checked(S_ALT_TAB),
        S_ALT_F4 => app.state.settings.disable_alt_f4 = ui.checked(S_ALT_F4),
        S_SLEEP if code == CBN_SELCHANGE => {
            if let Some((_, v)) = ui.sel(S_SLEEP, true).and_then(|i| SLEEP_CHOICES.get(i)) {
                app.state.settings.sleep = *v;
            }
        }
        P_LIST if code == LBN_SELCHANGE => {
            app.profile_sel = ui.sel(P_LIST, false);
            if let Some(n) = app.selected_profile() {
                set_text(ui.h(P_NAME), &n);
            }
        }
        P_SAVE => {
            let name = get_text(ui.h(P_NAME));
            app.save_profile(&name);
        }
        P_LOAD => match app.selected_profile() {
            Some(n) => return Some(Later::ConfirmLoad(n)),
            None => app.status = (Tone::Err, app.t(T::PickProfile).into()),
        },
        P_DELETE => match app.selected_profile() {
            Some(n) => return Some(Later::ConfirmDelete(n)),
            None => app.status = (Tone::Err, app.t(T::PickProfile).into()),
        },
        P_IMPORT => return Some(Later::Import),
        P_EXPORT => return Some(Later::Export),
        P_FOLDER => {
            let dir = state::profile_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
        P_FACTORY => return Some(Later::ConfirmFactory),
        P_CHECK => {
            scan(g, true);
            g.app.status = match g.app.link {
                Some(l) => (Tone::Ok, format!("{} {}", g.app.t(T::Found), g.app.link_name(l))),
                None => (Tone::Err, g.app.t(T::ErrNotFound).into()),
            };
        }
        _ => {}
    }
    None
}

fn on_slider(g: &mut G, h: HWND) {
    use id::*;
    let ui = &g.ui;
    let pos = send(h, TBM_GETPOS, 0, 0).clamp(0, 255) as u8;
    let app = &mut g.app;
    if h == ui.h(L_BRIGHT) {
        app.lighting.brightness = pos;
    } else if h == ui.h(L_SPEED) {
        app.lighting.speed = pos;
    } else if h == ui.h(L_DIR) {
        app.lighting.direction = pos;
    } else if h == ui.h(C_BRIGHT) {
        app.key_brightness = pos;
    } else if h == ui.h(S_RESP) {
        app.state.settings.response = pos.clamp(1, 5);
    }
}

fn confirm(owner: HWND, lang: Lang, text: T, extra: &str) -> bool {
    let body = if extra.is_empty() { tr(lang, text).to_string() } else { format!("{}\r\n\r\n{extra}", tr(lang, text)) };
    let body = wide(&body);
    let title = wide(tr(lang, T::AppTitle));
    unsafe { MessageBoxW(Some(owner), PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) == IDYES }
}

fn pick_color(owner: HWND, start: Rgb) -> Option<Rgb> {
    let mut custom = [COLORREF(0x00FF_FFFF); 16];
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: owner,
        rgbResult: kbview::colorref(start),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN,
        ..Default::default()
    };
    unsafe {
        if ChooseColorW(&mut cc).as_bool() {
            let v = cc.rgbResult.0;
            Some(Rgb::new(v as u8, (v >> 8) as u8, (v >> 16) as u8))
        } else {
            None
        }
    }
}

fn file_dialog(owner: HWND, save: bool) -> Option<std::path::PathBuf> {
    let mut buf = vec![0u16; 1024];
    if save {
        for (i, c) in "m87-profile.txt".encode_utf16().enumerate() {
            buf[i] = c;
        }
    }
    let filter: Vec<u16> = "Text (*.txt)\0*.txt\0All files (*.*)\0*.*\0\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: w!("txt"),
        Flags: if save { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR },
        ..Default::default()
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
    if !ok.as_bool() {
        return None;
    }
    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Run a modal step, then give the answer to the model.
fn run_later(hwnd: HWND, later: Later) {
    let lang = with_g(|g| g.app.lang).unwrap_or(Lang::En);
    match later {
        Later::PickColor { brush, start } => {
            if let Some(c) = pick_color(hwnd, start) {
                with_g(|g| {
                    if brush {
                        g.app.brush = c;
                    } else {
                        g.app.set_effect_color(c);
                    }
                });
            }
        }
        Later::ConfirmLoad(name) => {
            if confirm(hwnd, lang, T::ConfirmLoad, &name) {
                with_g(|g| start_job(g, Job::LoadProfile(name)));
            }
        }
        Later::ConfirmDelete(name) => {
            if confirm(hwnd, lang, T::ConfirmDelete, &name) {
                with_g(|g| g.app.delete_profile(&name));
            }
        }
        Later::ConfirmFactory => {
            if confirm(hwnd, lang, T::ConfirmFactory, "") {
                with_g(|g| start_job(g, Job::FactoryReset));
            }
        }
        Later::Import => {
            if let Some(p) = file_dialog(hwnd, false) {
                with_g(|g| g.app.import_profile(&p));
            }
        }
        Later::Export => {
            if let Some(p) = file_dialog(hwnd, true) {
                with_g(|g| g.app.export_profile(&p));
            }
        }
    }
    with_g(sync);
}

// ---------------------------------------------------------------- drawing

fn draw_item(g: Option<&G>, dis: &DRAWITEMSTRUCT) {
    use id::*;
    let cid = dis.CtlID as u16;
    let h = dis.hwndItem;
    if dis.CtlType == ODT_LISTBOX {
        if dis.itemID == u32::MAX {
            return;
        }
        let n = send(h, LB_GETTEXTLEN, dis.itemID as usize, 0).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = send(h, LB_GETTEXT, dis.itemID as usize, buf.as_mut_ptr() as isize).max(0) as usize;
        let text = String::from_utf16_lossy(&buf[..got.min(n)]);
        let enabled = unsafe { IsWindowEnabled(h).as_bool() };
        theme::draw_list_item(dis, &text, enabled);
        return;
    }
    let label = get_text(h);
    if (NAV..NAV + Page::ALL.len() as u16).contains(&cid) {
        let i = (cid - NAV) as usize;
        let glyph = [glyph::LIGHTING, glyph::KEY_COLORS, glyph::KEYS, glyph::MACROS, glyph::SETTINGS, glyph::PROFILES, glyph::ABOUT][i];
        let selected = SHOWN.with(|s| s.get()) as usize == i;
        theme::draw_button(dis, Look::Nav { glyph, selected }, &label, theme::BG);
        return;
    }
    let look = if (L_PALETTE..L_PALETTE + 9).contains(&cid) || (C_PALETTE..C_PALETTE + 10).contains(&cid) {
        // Swatches need the model to know which one is chosen.
        let Some(g) = g else { return };
        let app = &g.app;
        if cid < C_PALETTE {
            let c = PALETTE[(cid - L_PALETTE) as usize].1;
            Look::Swatch { color: c, selected: !app.lighting.rainbow && app.lighting.color == c }
        } else {
            let c = PALETTE[(cid - C_PALETTE) as usize].1;
            Look::Swatch { color: c, selected: app.brush == c }
        }
    } else if cid == L_CUSTOM || cid == C_CUSTOM {
        let Some(g) = g else { return };
        let c = if cid == L_CUSTOM { g.app.lighting.color } else { g.app.brush };
        let text = format!("{}  #{}", tr(g.app.lang, T::Custom), c.to_hex());
        theme::draw_button(dis, Look::ColorButton(c), &text, theme::CARD);
        return;
    } else if [L_APPLY, C_APPLY, K_APPLY, S_APPLY, M_SAVE].contains(&cid) {
        Look::Primary
    } else {
        Look::Secondary
    };
    theme::draw_button(dis, look, &label, theme::CARD);
}

fn paint_view(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right, rc.bottom);
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, bmp.into());
        let mode = if GetDlgCtrlID(hwnd) == id::C_VIEW as i32 { Mode::Colors } else { Mode::Mapping };
        let drawn = G_CELL.with(|c| match c.try_borrow() {
            Ok(b) => match b.as_ref() {
                Some(g) => {
                    kbview::paint(mem, w, h, &g.app, mode, g.ui.small, theme::CARD);
                    true
                }
                None => false,
            },
            Err(_) => false,
        });
        if !drawn {
            let br = CreateSolidBrush(kbview::colorref(theme::CARD));
            FillRect(mem, &rc, br);
            let _ = DeleteObject(br.into());
        }
        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

// ---------------------------------------------------------------- window procedures

extern "system" fn view_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                paint_view(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_MOUSEMOVE => {
                let dragging = msg == WM_MOUSEMOVE;
                if dragging && (wp.0 & MK_LBUTTON == 0 || GetCapture() != hwnd) {
                    return LRESULT(0);
                }
                let cid = GetDlgCtrlID(hwnd);
                if !dragging {
                    SetCapture(hwnd);
                    DRAG_LAST.with(|d| d.set(None));
                } else if cid != id::C_VIEW as i32 {
                    return LRESULT(0);
                }
                let x = (lp.0 & 0xFFFF) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                if let Some(i) = kbview::hit(rc.right, rc.bottom, x, y) {
                    if DRAG_LAST.with(|d| d.replace(Some(i))) != Some(i) {
                        if let Ok(parent) = GetParent(hwnd) {
                            post(parent.0 as isize, WM_APP_KEY, i as usize, cid as isize);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let cid = (wp.0 & 0xFFFF) as u16;
                let code = ((wp.0 >> 16) & 0xFFFF) as u32;
                // Lists, combo boxes and text boxes send many notices (focus,
                // drop-down closed, text changed). Only "selection changed"
                // matters; reacting to the others would put the old selection
                // back before the change arrives.
                let wanted = match cid {
                    id::LANG | id::K_MOUSE | id::K_MEDIA | id::K_MACRO | id::S_SLEEP => code == CBN_SELCHANGE,
                    id::L_EFFECTS | id::M_LIST | id::P_LIST => code == LBN_SELCHANGE,
                    id::M_STEPS | id::P_NAME => false,
                    _ => true,
                };
                if !wanted {
                    return DefWindowProcW(hwnd, msg, wp, lp);
                }
                let later = with_g(|g| {
                    let l = on_command(g, cid, code);
                    sync(g);
                    l
                });
                if let Some(Some(l)) = later {
                    run_later(hwnd, l);
                }
                LRESULT(0)
            }
            WM_NOTIFY => {
                let hdr = &*(lp.0 as *const NMHDR);
                if hdr.code == TTN_GETDISPINFOW {
                    // Tooltip of a color swatch: its window text is the color name.
                    let info = &mut *(lp.0 as *mut NMTTDISPINFOW);
                    let name: Vec<u16> = get_text(HWND(hdr.idFrom as *mut c_void)).encode_utf16().take(79).collect();
                    info.szText = [0; 80];
                    info.szText[..name.len()].copy_from_slice(&name);
                    info.lpszText = PWSTR(info.szText.as_mut_ptr());
                }
                LRESULT(0)
            }
            WM_HSCROLL => {
                let h = HWND(lp.0 as *mut c_void);
                with_g(|g| {
                    on_slider(g, h);
                    sync(g);
                });
                LRESULT(0)
            }
            WM_APP_KEY => {
                with_g(|g| {
                    let i = wp.0 as u8;
                    if lp.0 == id::C_VIEW as isize {
                        g.app.paint(i);
                    } else {
                        g.app.click_key(i);
                    }
                    sync(g);
                });
                LRESULT(0)
            }
            WM_APP_DONE => {
                let result = *Box::from_raw(lp.0 as *mut Result<Link, HidError>);
                with_g(|g| {
                    g.app.finish(result);
                    sync(g);
                });
                LRESULT(0)
            }
            WM_APP_BATTERY => {
                with_g(|g| {
                    g.ui.battery_running = false;
                    if wp.0 <= 100 {
                        g.app.battery = Some(wp.0 as u8);
                    }
                    sync(g);
                });
                LRESULT(0)
            }
            WM_TIMER => {
                with_g(|g| {
                    scan(g, wp.0 == TIMER_BATTERY);
                    sync(g);
                });
                LRESULT(0)
            }
            WM_DRAWITEM => {
                let dis = &*(lp.0 as *const DRAWITEMSTRUCT);
                G_CELL.with(|c| match c.try_borrow() {
                    Ok(b) => draw_item(b.as_ref(), dis),
                    Err(_) => draw_item(None, dis),
                });
                LRESULT(1)
            }
            WM_MEASUREITEM => {
                let mis = &mut *(lp.0 as *mut MEASUREITEMSTRUCT);
                mis.itemHeight = theme::s(LIST_ROW) as u32;
                LRESULT(1)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let [l, t, r, b] = CARD.map(theme::s);
                theme::card(hdc, RECT { left: l, top: t, right: r, bottom: b });
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CTLCOLORSTATIC => {
                // Decided from the control id and the colors saved by `sync`,
                // because a label can repaint while the model is in use.
                let hdc = HDC(wp.0 as *mut c_void);
                let cid = GetDlgCtrlID(HWND(lp.0 as *mut c_void)) as u16;
                let on_card = ![id::CONN, id::STATUS, id::TITLE, id::LANG_LBL].contains(&cid);
                let (status, conn) = LABEL_FG.with(|c| c.get());
                let fg = match cid {
                    id::STATUS => status,
                    id::CONN => conn,
                    id::LANG_LBL => theme::TEXT_DIM,
                    id::L_UNSENT | id::C_UNSENT | id::K_UNSENT | id::S_UNSENT => theme::NOTE_TEXT,
                    _ => theme::TEXT,
                };
                SetTextColor(hdc, kbview::colorref(fg));
                SetBkColor(hdc, kbview::colorref(if on_card { theme::CARD } else { theme::BG }));
                LRESULT(if on_card { theme::brush_card() } else { theme::brush_bg() }.0 as isize)
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                with_g(|g| {
                    g.ui.dpi = (wp.0 & 0xFFFF) as u32;
                    g.ui.layout();
                    sync(g);
                });
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

// ---------------------------------------------------------------- start

pub fn run() -> windows::core::Result<()> {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        });
        theme::init();
        let hinst = HINSTANCE(GetModuleHandleW(None)?.0);
        let cursor = LoadCursorW(None, IDC_ARROW)?;
        let icon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let main_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_proc),
            hInstance: hinst,
            hCursor: cursor,
            hIcon: icon,
            hbrBackground: theme::brush_bg(),
            lpszClassName: w!("M87ProSettingsWindow"),
            ..Default::default()
        };
        RegisterClassExW(&main_class);
        let view_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(view_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or(cursor),
            lpszClassName: w!("M87KeyboardView"),
            ..Default::default()
        };
        RegisterClassExW(&view_class);

        let lang = load_lang();
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let ex = WS_EX_CONTROLPARENT;
        let dpi = GetDpiForSystem();
        let mut rc = RECT { left: 0, top: 0, right: CLIENT_W * dpi as i32 / 96, bottom: CLIENT_H * dpi as i32 / 96 };
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, dpi);
        let title = wide(tr(lang, T::AppTitle));
        let hwnd = CreateWindowExW(
            ex,
            w!("M87ProSettingsWindow"),
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rc.right - rc.left,
            rc.bottom - rc.top,
            None,
            None,
            Some(hinst),
            None,
        )?;
        // Title bar in the window color (Windows 11; ignored elsewhere).
        let caption = kbview::colorref(theme::BG);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &caption as *const _ as *const c_void, 4);
        let dpi_now = GetDpiForWindow(hwnd);
        if dpi_now != dpi {
            let mut rc = RECT { left: 0, top: 0, right: CLIENT_W * dpi_now as i32 / 96, bottom: CLIENT_H * dpi_now as i32 / 96 };
            let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, dpi_now);
            let _ = SetWindowPos(hwnd, None, 0, 0, rc.right - rc.left, rc.bottom - rc.top, SWP_NOMOVE | SWP_NOZORDER);
        }

        let mut ui = Ui {
            main: hwnd,
            lang,
            dpi: dpi_now,
            font: HFONT::default(),
            bold: HFONT::default(),
            mono: HFONT::default(),
            small: HFONT::default(),
            ctls: Vec::new(),
            by_id: HashMap::new(),
            items: HashMap::new(),
            macro_loaded: None,
            battery_running: false,
            tip: HWND::default(),
        };
        ui.build();
        ui.layout();
        ui.apply_language(lang);
        let app = App::load(lang);
        ui.show_page(app.page);
        G_CELL.with(|c| *c.borrow_mut() = Some(G { app, ui }));
        with_g(|g| {
            scan(g, true);
            sync(g);
        });

        let _ = ShowWindow(hwnd, SW_SHOW);

        #[cfg(debug_assertions)]
        {
            let args: Vec<String> = std::env::args().collect();
            if let Some(i) = args.iter().position(|a| a == "--selftest") {
                // Switch the language the way a user does, and report.
                let out = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "selftest.txt".into()));
                let mut log = String::new();
                let combo = with_g(|g| g.ui.h(id::LANG)).unwrap_or_default();
                for target in [0usize, 1, 0] {
                    send(combo, CB_SETCURSEL, target, 0);
                    // Order seen when a user picks an item with the mouse.
                    for code in [9usize, 8, CBN_SELCHANGE as usize] {
                        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(id::LANG as usize | code << 16)), Some(LPARAM(combo.0 as isize)));
                    }
                    let title = get_text(hwnd);
                    let apply = with_g(|g| get_text(g.ui.h(id::L_APPLY))).unwrap_or_else(|| "<busy>".into());
                    let lang = with_g(|g| g.app.lang);
                    log += &format!("select {target}: lang={lang:?} title={title} apply={apply}
");
                }
                let _ = std::fs::write(out, log);
                let _ = DestroyWindow(hwnd);
                return Ok(());
            }
            if let Some(i) = args.iter().position(|a| a == "--capture") {
                let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| ".".into()));
                let _ = std::fs::create_dir_all(&dir);
                for l in Lang::ALL {
                    with_g(|g| {
                        g.app.lang = l;
                        g.app.status = (Tone::Info, tr(l, T::Ready).into());
                        g.ui.apply_language(l);
                        // Sample content so every control has something to show.
                        g.app.map_key = m87_proto::layout::find("CapsLock");
                        g.app.paint_all(Rgb::new(0, 0x80, 0xFF));
                        g.app.paint(1);
                    });
                    for p in Page::ALL {
                        with_g(|g| {
                            g.app.page = p;
                            g.ui.show_page(p);
                            sync(g);
                        });
                        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW);
                        for _ in 0..3 {
                            let mut m = MSG::default();
                            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                                let _ = TranslateMessage(&m);
                                DispatchMessageW(&m);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(150));
                        }
                        crate::capture::save(hwnd, &dir.join(format!("{}-{}.png", l.code(), p as u8 + 1)));
                    }
                }
                let _ = DestroyWindow(hwnd);
                return Ok(());
            }
        }

        SetTimer(Some(hwnd), TIMER_SCAN, 3000, None);
        SetTimer(Some(hwnd), TIMER_BATTERY, 60_000, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if IsDialogMessageW(hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
