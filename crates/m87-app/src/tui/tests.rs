//! Render every page into a test terminal and click through the main flows.

use super::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| draw(f, app)).unwrap();
    let buf = t.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..h {
        for x in 0..w {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push(char::from(10));
    }
    out
}

fn click(app: &mut App, hit: Hit) {
    let (r, _) = *app.hits.iter().find(|(_, h)| *h == hit).unwrap_or_else(|| panic!("{hit:?} not on screen"));
    let h = app.hit_at(r.x, r.y).unwrap();
    app.on_hit(h);
}

fn app() -> App {
    let mut app = App::blank();
    app.link = Some(Link::Dongle);
    app.battery = Some(92);
    app
}

#[test]
fn lighting_and_colors() {
    let mut app = app();
    let s = render(&mut app, 100, 30);
    assert!(s.contains("Battery: 92%"));
    click(&mut app, Hit::Effect(7));
    assert_eq!(app.lighting.effect, Effect::Breath);
    render(&mut app, 100, 30);
    click(&mut app, Hit::Palette(Target::Effect, 5));
    assert_eq!(app.lighting.color, Rgb::new(0, 0, 0xFF));

    app.tab = Tab::KeyColors;
    render(&mut app, 100, 30);
    click(&mut app, Hit::PaintKey(1));
    assert_eq!(app.key_colors[1], app.brush);
}

#[test]
fn key_mapping_flow() {
    let mut app = app();
    app.tab = Tab::Keys;
    let s = render(&mut app, 100, 30);
    assert!(s.contains("Click a key above"), "{s}");
    // Select CapsLock (index 55), choose key A (index 56), add Ctrl.
    click(&mut app, Hit::MapKey(55));
    render(&mut app, 100, 30);
    click(&mut app, Hit::ChooseKey);
    render(&mut app, 100, 30);
    click(&mut app, Hit::MapKey(56));
    assert_eq!(app.state.key(Layer::Base, "CapsLock"), KeyAction::Key { modifiers: 0, usage: 0x04 });
    render(&mut app, 100, 30);
    click(&mut app, Hit::Modifier(m87_proto::keymap::modifier::LCTRL));
    assert_eq!(app.state.key(Layer::Base, "CapsLock").to_text(), "key:Ctrl+A");
    let s = render(&mut app, 100, 30);
    println!("{s}");
    assert!(s.contains("Changes not sent yet"));
    // Fn layer, media.
    click(&mut app, Hit::Layer(Layer::Fn));
    render(&mut app, 100, 30);
    click(&mut app, Hit::SetAction(KeyAction::Media(0xE2)));
    assert_eq!(app.state.key(Layer::Fn, "CapsLock"), KeyAction::Media(0xE2));
}

#[test]
fn macro_editing() {
    let mut app = app();
    app.tab = Tab::Macros;
    render(&mut app, 100, 30);
    click(&mut app, Hit::MacroSelect(2));
    render(&mut app, 100, 30);
    click(&mut app, Hit::MacroInput);
    assert!(app.macro_editing);
    for c in "tap:H, tap:I".chars() {
        app.on_key(KeyCode::Char(c), KeyModifiers::NONE);
    }
    // 'q' while typing must not quit.
    assert!(!app.quit);
    app.on_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.state.macros.contains_key(&2));
    assert_eq!(app.pending, Some(Pending::Page(Tab::Macros)));
    let s = render(&mut app, 100, 30);
    println!("{s}");
    assert!(s.contains("tap:H, tap:I"));

    // Bad text gives a clear error and keeps the text.
    click(&mut app, Hit::MacroInput);
    app.on_key(KeyCode::Char('x'), KeyModifiers::NONE);
    app.on_key(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.status.0, Tone::Err);
}

#[test]
fn settings_page() {
    let mut app = app();
    app.tab = Tab::Settings;
    render(&mut app, 100, 30);
    click(&mut app, Hit::Toggle(0));
    assert!(app.state.settings.disable_win);
    click(&mut app, Hit::Sleep(3));
    assert_eq!(app.state.settings.sleep, 3);
    let s = render(&mut app, 100, 30);
    println!("{s}");
    assert!(s.contains("Changes not sent yet"));
    app.tab = Tab::Device;
    render(&mut app, 100, 30);
    assert!(render(&mut app, 60, 20).contains("too small"));
}

#[test]
fn keyboard_shortcuts() {
    let mut app = app();
    // Lighting: Up/Down change the effect, Left/Right the brightness.
    let effect = app.lighting.effect;
    app.on_key(KeyCode::Down, KeyModifiers::NONE);
    assert_ne!(app.lighting.effect, effect);
    let b = app.lighting.brightness;
    app.on_key(KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(app.lighting.brightness, b - 1);
    // Key mapping: arrows walk the keyboard picture.
    app.on_key(KeyCode::Char('3'), KeyModifiers::NONE);
    assert!(render(&mut app, 100, 30).contains("Arrows select key"));
    app.on_key(KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(app.map_key.unwrap().id, "Esc");
    app.on_key(KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(app.map_key.unwrap().id, "F1");
    app.on_key(KeyCode::Down, KeyModifiers::NONE);
    app.on_key(KeyCode::Down, KeyModifiers::NONE);
    app.on_key(KeyCode::Left, KeyModifiers::NONE);
    app.on_key(KeyCode::Up, KeyModifiers::NONE);
    assert!(app.map_key.is_some());
    // Macros: Up/Down select.
    app.on_key(KeyCode::Char('4'), KeyModifiers::NONE);
    app.on_key(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(app.macro_sel, 2);
    // Device page does not advertise Enter.
    app.on_key(KeyCode::Char('6'), KeyModifiers::NONE);
    assert!(!render(&mut app, 100, 30).contains("Enter"));
}

#[test]
fn same_keyboard_on_both_pages() {
    for (w, h) in [(80, 24), (90, 26), (100, 30), (110, 30), (120, 30), (120, 40)] {
        let mut app = app();
        app.tab = Tab::KeyColors;
        let colors = render(&mut app, w, h);
        let color_line = colors.lines().position(|l| l.contains("Color ")).unwrap();
        let paint: Vec<Rect> = app.hits.iter().filter(|(_, h)| matches!(h, Hit::PaintKey(_))).map(|(r, _)| *r).collect();
        app.tab = Tab::Keys;
        app.map_key = KEYS.iter().find(|k| k.id == "CapsLock");
        let s = render(&mut app, w, h);
        let map: Vec<Rect> = app.hits.iter().filter(|(_, h)| matches!(h, Hit::MapKey(_))).map(|(r, _)| *r).collect();
        assert_eq!(paint, map, "{w}x{h}");
        assert_eq!(paint.len(), KEYS.len(), "{w}x{h}");
        // The whole action list and the Apply button still fit.
        assert!(app.hits.iter().any(|(_, h)| *h == Hit::Apply), "{w}x{h}
{s}");
        assert!(s.contains("Macro    No macros yet"), "{w}x{h}
{s}");
        // Same blank line under the keyboard as on Key colors, when it fits.
        if h >= 30 {
            let layer_line = s.lines().position(|l| l.contains("Layer ")).unwrap();
            assert_eq!(layer_line, color_line, "{w}x{h}
{s}");
        }
        if (w, h) == (80, 24) || (w, h) == (110, 30) {
            println!("{s}");
        }
    }
}

#[test]
fn pages_fit_80_columns() {
    let mut app = app();
    for t in Tab::ALL {
        app.tab = t;
        let s = render(&mut app, 80, 24);
        assert!(!s.contains("too small"), "{t:?}");
        assert!(s.contains("6 Device"), "tabs must fit in 80 columns");
    }
}
