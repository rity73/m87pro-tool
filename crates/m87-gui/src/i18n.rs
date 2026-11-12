//! English and Korean text. Every visible string goes through `tr`.

use m87_proto::keymap::Mouse;
use m87_proto::lighting::Effect;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Ko,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ko];

    /// Name shown in the language list (always in its own language).
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ko => "한국어",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ko => "ko",
        }
    }

    pub fn from_code(s: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == s.trim())
    }
}

macro_rules! strings {
    ($($name:ident => $en:expr, $ko:expr;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum T { $($name),* }

        pub fn tr(lang: Lang, t: T) -> &'static str {
            match (lang, t) {
                $((Lang::En, T::$name) => $en, (Lang::Ko, T::$name) => $ko,)*
            }
        }

        #[cfg(test)]
        pub const ALL_T: &[T] = &[$(T::$name),*];
    };
}

strings! {
    AppTitle => "M87 Pro Settings", "M87 Pro 설정";
    TabLighting => "Lighting", "조명";
    TabKeyColors => "Key colors", "키 색상";
    TabKeys => "Key mapping", "키 매핑";
    TabMacros => "Macros", "매크로";
    TabSettings => "Settings", "설정";
    TabProfiles => "Profiles", "프로필";
    ConnDongle => "Connected: 2.4 GHz receiver", "연결됨: 2.4GHz 수신기";
    ConnWired => "Connected: USB cable", "연결됨: USB 케이블";
    NotConnected => "Not connected. Connect the USB cable or plug in the 2.4 GHz receiver.", "연결 안 됨. USB 케이블을 연결하거나 2.4GHz 수신기를 꽂으세요.";
    Battery => "Battery", "배터리";
    Charging => "Charging", "충전 중";
    Language => "Language", "언어";
    LinkDongle => "2.4 GHz receiver", "2.4GHz 수신기";
    LinkWired => "USB cable", "USB 케이블";

    Apply => "Apply to keyboard", "키보드에 적용";
    Sending => "Sending to the keyboard...", "키보드로 보내는 중...";
    Unsent => "Changes not sent yet.", "아직 보내지 않은 변경이 있습니다.";
    Busy => "Wait until the current send finishes.", "보내는 중입니다. 끝날 때까지 기다려 주세요.";
    Ready => "Choose settings, then click \"Apply to keyboard\".", "설정을 고른 뒤 [키보드에 적용]을 누르세요.";
    NotApplied => "Not applied:", "적용하지 못했습니다:";
    Applied => "Applied over the", "적용했습니다. 연결:";
    Cancelled => "Cancelled.", "취소했습니다.";
    SaveFailed => "Could not save on this PC:", "이 PC에 저장하지 못했습니다:";
    LoadFailed => "Saved settings could not be read:", "저장된 설정을 읽지 못했습니다:";

    Effect => "Effect", "효과";
    Color => "Color", "색";
    Custom => "Custom...", "직접 고르기...";
    Rainbow => "All colors (rainbow)", "모든 색 (무지개)";
    Brightness => "Brightness", "밝기";
    Speed => "Speed", "속도";
    Direction => "Direction", "방향";
    Uses => "This effect uses:", "이 효과에 쓰이는 설정:";
    UsesNone => "This effect turns the lighting off.", "이 효과는 조명을 끕니다.";
    LedNote => "If the lighting does not change, press Fn+Backspace on the keyboard. That shortcut turns all lighting on and off, and this program cannot change the lighting while it is off. The light bar above the arrow keys can only be changed with keyboard shortcuts.", "조명이 바뀌지 않으면 키보드에서 Fn+Backspace를 누르세요. 이 단축키는 조명 전체를 켜고 끄며, 꺼져 있는 동안에는 이 프로그램으로 조명을 바꿀 수 없습니다. 방향키 위 LED 바는 키보드 단축키로만 바꿀 수 있습니다.";
    EffectSent => "Lighting effect sent.", "조명 효과를 보냈습니다.";

    BrushColor => "Brush color", "칠할 색";
    PaintAll => "Paint all keys", "모든 키 칠하기";
    AllOff => "Turn all keys off", "모든 키 끄기";
    KeysLit => "keys lit", "개 키 켜짐";
    KeyColorsHelp => "Click or drag over keys to paint them with the brush color.", "키를 클릭하거나 드래그하면 칠할 색으로 칠해집니다.";
    KeyColorsNote => "Shows the colors last sent from this PC. The keyboard cannot report its colors.", "이 PC에서 마지막으로 보낸 색을 보여 줍니다. 키보드는 현재 색을 알려 주지 않습니다.";
    KeyColorsSent => "Per-key colors sent.", "키별 색상을 보냈습니다.";

    Layer => "Layer", "레이어";
    LayerNormal => "Normal", "기본";
    LayerFn => "While Fn is held", "Fn을 누르고 있을 때";
    ChangedYellow => "Changed keys are yellow.", "바뀐 키는 노란색으로 표시됩니다.";
    ClickKey => "Click a key on the picture to change it.", "그림에서 바꿀 키를 클릭하세요.";
    Selected => "Selected key:", "선택한 키:";
    NowDoes => "now does:", "현재 동작:";
    FnFixed => "The Fn key cannot be changed.", "Fn 키는 바꿀 수 없습니다.";
    Basic => "Basic", "기본";
    NormalFunction => "Normal function", "원래 기능";
    DoNothing => "Do nothing", "동작 안 함";
    KeyRow => "Key", "키";
    ChooseKey => "Choose key...", "키 고르기...";
    Sends => "Sends", "보내는 키:";
    Mouse => "Mouse", "마우스";
    Media => "Media", "미디어";
    Macro => "Macro", "매크로";
    ChooseOne => "(choose)", "(선택)";
    NoMacros => "(no macros yet)", "(매크로 없음)";
    ResetAll => "Reset all keys to normal", "모든 키 원래대로";
    ChooseKeyHint => "Now click the key it should send. Press Esc to cancel.", "보낼 키를 클릭하세요. 취소하려면 Esc를 누르세요.";
    ModNeedsKey => "First click \"Choose key...\" and pick the key to send, then add Ctrl, Shift, Alt or Win.", "먼저 [키 고르기...]로 보낼 키를 고른 뒤 Ctrl, Shift, Alt, Win을 더하세요.";
    FirstClickKey => "First click the key you want to change.", "먼저 바꿀 키를 클릭하세요.";
    FnCannotSend => "Fn cannot be sent as a key. Choose another key.", "Fn은 보낼 수 없습니다. 다른 키를 고르세요.";
    NoKeyChanges => "There are no key changes to send.", "보낼 키 변경이 없습니다.";
    WillDo => "will do:", "새 동작:";
    ClickApply => "Click \"Apply to keyboard\" to send.", "[키보드에 적용]을 누르면 보냅니다.";
    KeysSent => "Key mapping sent.", "키 매핑을 보냈습니다.";
    ResetDone => "Every key does its normal function again.", "모든 키가 원래 기능으로 돌아왔습니다.";
    DisabledText => "nothing (disabled)", "없음 (비활성)";

    MacroList => "Macros (1-100)", "매크로 (1-100)";
    Empty => "(empty)", "(비어 있음)";
    StepsOf => "Steps of macro", "단계 - 매크로";
    SaveSend => "Save and send", "저장하고 보내기";
    Delete => "Delete", "삭제";
    MacroHelp => "Separate steps with commas. Steps:\r\n  tap:A          press and release a key\r\n  down:LShift    hold a key down\r\n  up:LShift      release a held key\r\n  click:left     click a mouse button (left, right, middle, back, forward)\r\n  wait:200       wait, in milliseconds\r\nExample: tap:H, tap:I, wait:100, tap:Enter", "단계는 쉼표로 나눕니다. 쓸 수 있는 단계:\r\n  tap:A          키를 눌렀다 뗌\r\n  down:LShift    키를 누른 채로 둠\r\n  up:LShift      누르고 있던 키를 뗌\r\n  click:left     마우스 버튼 클릭 (left, right, middle, back, forward)\r\n  wait:200       기다림 (밀리초)\r\n예: tap:H, tap:I, wait:100, tap:Enter";
    MacroUse => "To use a macro, give it to a key on the Key mapping page.", "매크로를 쓰려면 [키 매핑] 페이지에서 키에 연결하세요.";
    TypeSteps => "Type the steps first, or click Delete to remove this macro.", "먼저 단계를 입력하세요. 매크로를 지우려면 [삭제]를 누르세요.";
    MacroEmpty => "This macro is already empty.", "이 매크로는 이미 비어 있습니다.";
    MacroNotSaved => "Macro not saved:", "매크로를 저장하지 못했습니다:";
    MacrosSent => "All macros sent.", "모든 매크로를 보냈습니다.";

    BlockWin => "Block the Windows key", "Windows 키 막기";
    BlockAltTab => "Block Alt+Tab", "Alt+Tab 막기";
    BlockAltF4 => "Block Alt+F4", "Alt+F4 막기";
    Sleep => "Turn off after no use for", "사용하지 않으면 꺼지는 시간";
    Never => "Never", "끄지 않음";
    Min1 => "1 minute", "1분";
    Min5 => "5 minutes", "5분";
    Min30 => "30 minutes", "30분";
    Response => "Key response", "키 응답";
    ResponseNote => "1 = fastest. 5 = best protection against one press typing twice.", "1 = 가장 빠름. 5 = 한 번 눌렀는데 두 번 입력되는 것을 가장 잘 막음.";
    SettingsSent => "Settings sent.", "설정을 보냈습니다.";

    SavedProfiles => "Saved profiles", "저장된 프로필";
    Name => "Name", "이름";
    SaveCurrent => "Save current setup", "지금 설정 저장";
    SaveNote => "Saves the lighting, keys, macros and settings last sent to the keyboard.", "키보드에 마지막으로 보낸 조명, 키, 매크로, 설정을 저장합니다.";
    Load => "Load to keyboard", "키보드로 불러오기";
    LoadNote => "Loading replaces everything on the keyboard.", "불러오면 키보드의 모든 설정이 바뀝니다.";
    Import => "Import...", "가져오기...";
    Export => "Export...", "내보내기...";
    OpenFolder => "Open profile folder", "프로필 폴더 열기";
    FactoryTitle => "Factory reset", "공장 초기화";
    FactoryReset => "Factory reset...", "공장 초기화...";
    FactoryNote => "Default lighting, every key normal, no macros, default settings. Profiles on this PC are kept.", "기본 조명, 모든 키 원래대로, 매크로 없음, 기본 설정으로 돌아갑니다. 이 PC의 프로필은 남습니다.";
    ConfirmFactory => "Erase every change on the keyboard?\r\n\r\nLighting, key mapping, macros and settings go back to their defaults. Profiles saved on this PC are kept.", "키보드의 모든 변경을 지울까요?\r\n\r\n조명, 키 매핑, 매크로, 설정이 기본값으로 돌아갑니다. 이 PC에 저장된 프로필은 남습니다.";
    ConfirmDelete => "Delete this profile from this PC?", "이 프로필을 이 PC에서 지울까요?";
    ConfirmLoad => "Send this profile to the keyboard? It replaces the lighting, keys, macros and settings on the keyboard.", "이 프로필을 키보드로 보낼까요? 키보드의 조명, 키, 매크로, 설정이 모두 바뀝니다.";
    Connection => "Connection", "연결";
    CheckAgain => "Check again", "다시 확인";
    DataFolder => "Data is saved in", "데이터 저장 위치:";
    PickProfile => "First click a profile in the list.", "먼저 목록에서 프로필을 클릭하세요.";
    ProfileSaved => "Profile saved:", "프로필을 저장했습니다:";
    ProfileDeleted => "Profile deleted:", "프로필을 지웠습니다:";
    ProfileAdded => "Profile added:", "프로필을 추가했습니다:";
    Exported => "Exported to", "내보냈습니다:";
    ProfileLoaded => "Profile is now on the keyboard:", "프로필을 키보드에 적용했습니다:";
    BadName => "Use 1 to 40 letters, digits, spaces, - or _ for the profile name.", "프로필 이름은 1~40자의 글자, 숫자, 공백, -, _ 로 지으세요.";
    NotSaved => "Not saved:", "저장하지 못했습니다:";
    FactoryDone => "The keyboard is back to its factory settings.", "키보드를 공장 설정으로 되돌렸습니다.";
    ProfileFiles => "Profile files", "프로필 파일";
    Found => "Keyboard found:", "키보드를 찾았습니다:";

    ErrNotFound => "No M87 Pro keyboard or 2.4 GHz receiver was found. Connect the USB cable or plug in the receiver. Bluetooth connections cannot be configured.", "M87 Pro 키보드나 2.4GHz 수신기를 찾지 못했습니다. USB 케이블을 연결하거나 수신기를 꽂으세요. 블루투스 연결로는 설정할 수 없습니다.";
    ErrOffline => "The receiver is plugged in, but the keyboard is off, asleep or out of range. Turn it on or press a key to wake it, then retry.", "수신기는 꽂혀 있지만 키보드가 꺼져 있거나, 절전 중이거나, 너무 멀리 있습니다. 키보드를 켜거나 키를 눌러 깨운 뒤 다시 시도하세요.";
    ErrNoResponse => "The keyboard did not answer. Press a key to wake it, move it closer to the receiver, then retry.", "키보드가 응답하지 않았습니다. 키를 눌러 깨우고 수신기 가까이 둔 뒤 다시 시도하세요.";
    ErrOpen => "The keyboard was found, but its settings port could not be opened. Close the original M87 Pro software if it is running, then retry.", "키보드는 찾았지만 설정 포트를 열지 못했습니다. 원래 M87 Pro 프로그램이 실행 중이면 닫고 다시 시도하세요.";
    ErrIo => "USB communication failed. Unplug the keyboard or receiver, plug it in again and retry.", "USB 통신에 실패했습니다. 키보드나 수신기를 뺐다가 다시 꽂고 다시 시도하세요.";

    PalRed => "Red", "빨강";
    PalOrange => "Orange", "주황";
    PalYellow => "Yellow", "노랑";
    PalGreen => "Green", "초록";
    PalCyan => "Cyan", "청록";
    PalBlue => "Blue", "파랑";
    PalPurple => "Purple", "보라";
    PalPink => "Pink", "분홍";
    PalWhite => "White", "흰색";
    PalOff => "Off", "끄기";
}

/// Effect name for the list.
pub fn effect_name(lang: Lang, e: Effect) -> &'static str {
    let (en, ko) = match e {
        Effect::Off => ("Off", "끄기"),
        Effect::Static => ("Static", "고정"),
        Effect::SingleOn => ("Single on", "누른 키 켜짐"),
        Effect::SingleOff => ("Single off", "누른 키 꺼짐"),
        Effect::Glittering => ("Glittering", "반짝임"),
        Effect::Falling => ("Falling", "떨어짐"),
        Effect::Colourful => ("Colorful", "다채로움"),
        Effect::Breath => ("Breath", "숨쉬기"),
        Effect::Spectrum => ("Spectrum", "스펙트럼"),
        Effect::Outward => ("Outward", "퍼져 나감"),
        Effect::Scrolling => ("Scrolling", "스크롤"),
        Effect::Rolling => ("Rolling", "롤링"),
        Effect::Rotating => ("Rotating", "회전"),
        Effect::Explode => ("Explode", "폭발"),
        Effect::Launch => ("Launch", "발사"),
        Effect::Ripples => ("Ripples", "물결"),
        Effect::Flowing => ("Flowing", "흐름"),
        Effect::Pulsating => ("Pulsating", "맥박"),
        Effect::Tilt => ("Tilt", "기울기"),
        Effect::Shuttle => ("Shuttle", "왕복"),
    };
    pick(lang, en, ko)
}

pub fn mouse_name(lang: Lang, m: Mouse) -> &'static str {
    let (en, ko) = match m {
        Mouse::Left => ("Left click", "왼쪽 클릭"),
        Mouse::Right => ("Right click", "오른쪽 클릭"),
        Mouse::Middle => ("Middle click", "가운데 클릭"),
        Mouse::Back => ("Back button", "뒤로 버튼"),
        Mouse::Forward => ("Forward button", "앞으로 버튼"),
        Mouse::WheelUp => ("Wheel up", "휠 위로"),
        Mouse::WheelDown => ("Wheel down", "휠 아래로"),
    };
    pick(lang, en, ko)
}

/// Media function name by its CLI id (see `m87_proto::keymap::MEDIA`).
pub fn media_name(lang: Lang, id: &str) -> &'static str {
    let (en, ko) = match id {
        "play-pause" => ("Play / pause", "재생 / 일시정지"),
        "stop" => ("Stop", "정지"),
        "previous-track" => ("Previous track", "이전 곡"),
        "next-track" => ("Next track", "다음 곡"),
        "volume-up" => ("Volume up", "볼륨 크게"),
        "volume-down" => ("Volume down", "볼륨 작게"),
        "mute" => ("Mute", "음소거"),
        "media-player" => ("Open media player", "미디어 플레이어 열기"),
        "mail" => ("Open mail", "메일 열기"),
        "calculator" => ("Open calculator", "계산기 열기"),
        "file-explorer" => ("Open File Explorer", "파일 탐색기 열기"),
        "browser-home" => ("Browser home", "브라우저 홈"),
        "browser-search" => ("Browser search", "브라우저 검색"),
        "browser-back" => ("Browser back", "브라우저 뒤로"),
        "browser-forward" => ("Browser forward", "브라우저 앞으로"),
        "browser-refresh" => ("Browser refresh", "브라우저 새로 고침"),
        "browser-favorites" => ("Browser favorites", "브라우저 즐겨찾기"),
        _ => ("?", "?"),
    };
    pick(lang, en, ko)
}

fn pick(lang: Lang, en: &'static str, ko: &'static str) -> &'static str {
    match lang {
        Lang::En => en,
        Lang::Ko => ko,
    }
}

/// Language of the Windows display: Korean when the user's UI language is
/// Korean, otherwise English.
pub fn system_lang() -> Lang {
    // LANG_KOREAN = 0x12 in the low 10 bits of the LANGID.
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    if id & 0x3FF == 0x12 {
        Lang::Ko
    } else {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m87_proto::keymap::MEDIA;

    #[test]
    fn every_string_has_both_languages() {
        for t in ALL_T {
            for l in Lang::ALL {
                assert!(!tr(l, *t).is_empty(), "{t:?} {l:?}");
            }
        }
        for (id, _) in MEDIA {
            assert_ne!(media_name(Lang::Ko, id), "?", "{id}");
        }
    }
}
