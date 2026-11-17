//! Friendly key and mouse-button names used by the configuration file.

use evdev::KeyCode;
use std::str::FromStr;

/// Parse a keyboard key name into an evdev key code.
///
/// Accepts friendly names (`ctrl`, `pageup`, `f13`, `a`, `7`, ...), canonical
/// Linux names (`KEY_A`, ...) and a few common aliases.
pub fn parse_key(name: &str) -> Result<KeyCode, String> {
    let original = name.trim();
    if original.is_empty() {
        return Err("empty key name".to_string());
    }

    let norm = normalize(original);

    let friendly = match norm.as_str() {
        "ctrl" | "control" => Some(KeyCode::KEY_LEFTCTRL),
        "rightctrl" | "rightcontrol" => Some(KeyCode::KEY_RIGHTCTRL),
        "alt" | "option" => Some(KeyCode::KEY_LEFTALT),
        "altgr" | "rightalt" => Some(KeyCode::KEY_RIGHTALT),
        "shift" => Some(KeyCode::KEY_LEFTSHIFT),
        "rightshift" => Some(KeyCode::KEY_RIGHTSHIFT),
        "super" | "meta" | "win" | "windows" | "logo" | "cmd" | "command" => {
            Some(KeyCode::KEY_LEFTMETA)
        }
        "rightsuper" | "rightmeta" | "rightwin" => Some(KeyCode::KEY_RIGHTMETA),
        "enter" | "return" => Some(KeyCode::KEY_ENTER),
        "esc" | "escape" => Some(KeyCode::KEY_ESC),
        "space" | "spacebar" => Some(KeyCode::KEY_SPACE),
        "tab" => Some(KeyCode::KEY_TAB),
        "backspace" | "bksp" => Some(KeyCode::KEY_BACKSPACE),
        "delete" | "del" => Some(KeyCode::KEY_DELETE),
        "insert" | "ins" => Some(KeyCode::KEY_INSERT),
        "home" => Some(KeyCode::KEY_HOME),
        "end" => Some(KeyCode::KEY_END),
        "pageup" | "pgup" | "prior" => Some(KeyCode::KEY_PAGEUP),
        "pagedown" | "pgdn" | "next" => Some(KeyCode::KEY_PAGEDOWN),
        "up" | "arrowup" => Some(KeyCode::KEY_UP),
        "down" | "arrowdown" => Some(KeyCode::KEY_DOWN),
        "left" | "arrowleft" => Some(KeyCode::KEY_LEFT),
        "right" | "arrowright" => Some(KeyCode::KEY_RIGHT),
        "menu" | "contextmenu" => Some(KeyCode::KEY_MENU),
        "pause" => Some(KeyCode::KEY_PAUSE),
        "printscreen" | "print" | "sysrq" => Some(KeyCode::KEY_SYSRQ),
        "scrolllock" => Some(KeyCode::KEY_SCROLLLOCK),
        "capslock" | "caps" => Some(KeyCode::KEY_CAPSLOCK),
        "numlock" => Some(KeyCode::KEY_NUMLOCK),
        "minus" | "dash" => Some(KeyCode::KEY_MINUS),
        "equal" | "equals" => Some(KeyCode::KEY_EQUAL),
        "leftbrace" | "bracketleft" | "[" => Some(KeyCode::KEY_LEFTBRACE),
        "rightbrace" | "bracketright" | "]" => Some(KeyCode::KEY_RIGHTBRACE),
        "backslash" | "\\" => Some(KeyCode::KEY_BACKSLASH),
        "semicolon" | ";" => Some(KeyCode::KEY_SEMICOLON),
        "apostrophe" | "quote" | "'" => Some(KeyCode::KEY_APOSTROPHE),
        "grave" | "backtick" | "`" => Some(KeyCode::KEY_GRAVE),
        "comma" | "," => Some(KeyCode::KEY_COMMA),
        "dot" | "period" | "." => Some(KeyCode::KEY_DOT),
        "slash" | "/" => Some(KeyCode::KEY_SLASH),
        "mute" => Some(KeyCode::KEY_MUTE),
        "volumedown" | "voldown" => Some(KeyCode::KEY_VOLUMEDOWN),
        "volumeup" | "volup" => Some(KeyCode::KEY_VOLUMEUP),
        "playpause" | "play" => Some(KeyCode::KEY_PLAYPAUSE),
        "nextsong" | "nexttrack" => Some(KeyCode::KEY_NEXTSONG),
        "previoussong" | "prevtrack" => Some(KeyCode::KEY_PREVIOUSSONG),
        "stop" => Some(KeyCode::KEY_STOPCD),
        _ => None,
    };
    if let Some(code) = friendly {
        return Ok(code);
    }

    // f1 .. f24. F1..F12 and F13..F24 are not one contiguous range.
    if let Some(rest) = norm.strip_prefix('f')
        && let Ok(n) = rest.parse::<u8>()
        && (1..=24).contains(&n)
    {
        let code = match n {
            1 => KeyCode::KEY_F1,
            2 => KeyCode::KEY_F2,
            3 => KeyCode::KEY_F3,
            4 => KeyCode::KEY_F4,
            5 => KeyCode::KEY_F5,
            6 => KeyCode::KEY_F6,
            7 => KeyCode::KEY_F7,
            8 => KeyCode::KEY_F8,
            9 => KeyCode::KEY_F9,
            10 => KeyCode::KEY_F10,
            11 => KeyCode::KEY_F11,
            12 => KeyCode::KEY_F12,
            13 => KeyCode::KEY_F13,
            14 => KeyCode::KEY_F14,
            15 => KeyCode::KEY_F15,
            16 => KeyCode::KEY_F16,
            17 => KeyCode::KEY_F17,
            18 => KeyCode::KEY_F18,
            19 => KeyCode::KEY_F19,
            20 => KeyCode::KEY_F20,
            21 => KeyCode::KEY_F21,
            22 => KeyCode::KEY_F22,
            23 => KeyCode::KEY_F23,
            24 => KeyCode::KEY_F24,
            _ => unreachable!(),
        };
        return Ok(code);
    }

    // num0 .. num9
    if let Some(rest) = norm.strip_prefix("num")
        && rest.len() == 1
        && rest.as_bytes()[0].is_ascii_digit()
    {
        let digit = rest.as_bytes()[0];
        let code = match digit {
            b'0' => KeyCode::KEY_KP0,
            b'1' => KeyCode::KEY_KP1,
            b'2' => KeyCode::KEY_KP2,
            b'3' => KeyCode::KEY_KP3,
            b'4' => KeyCode::KEY_KP4,
            b'5' => KeyCode::KEY_KP5,
            b'6' => KeyCode::KEY_KP6,
            b'7' => KeyCode::KEY_KP7,
            b'8' => KeyCode::KEY_KP8,
            b'9' => KeyCode::KEY_KP9,
            _ => unreachable!(),
        };
        return Ok(code);
    }

    // Single letters and digits.
    if norm.len() == 1 {
        let b = norm.as_bytes()[0];
        if b.is_ascii_lowercase() {
            let upper = norm.to_ascii_uppercase();
            return KeyCode::from_str(&format!("KEY_{upper}"))
                .map_err(|_| format!("unknown key {original:?}"));
        }
        if b.is_ascii_digit() {
            let code = match b {
                b'0' => KeyCode::KEY_0,
                b'1' => KeyCode::KEY_1,
                b'2' => KeyCode::KEY_2,
                b'3' => KeyCode::KEY_3,
                b'4' => KeyCode::KEY_4,
                b'5' => KeyCode::KEY_5,
                b'6' => KeyCode::KEY_6,
                b'7' => KeyCode::KEY_7,
                b'8' => KeyCode::KEY_8,
                b'9' => KeyCode::KEY_9,
                _ => unreachable!(),
            };
            return Ok(code);
        }
    }

    // Canonical evdev name, case-insensitive (`KEY_A`, `BTN_LEFT`, ...).
    let canonical = original.to_ascii_uppercase();
    KeyCode::from_str(&canonical).map_err(|_| format!("unknown key {original:?}"))
}

/// Parse a mouse-button name used as the *output* of a mapping.
pub fn parse_mouse_button(name: &str) -> Result<KeyCode, String> {
    let original = name.trim();
    if original.is_empty() {
        return Err("empty mouse button name".to_string());
    }
    let norm = normalize(original);
    let code = match norm.as_str() {
        "left" | "l" | "primary" => KeyCode::BTN_LEFT,
        "right" | "r" | "secondary" => KeyCode::BTN_RIGHT,
        "middle" | "m" | "wheel" => KeyCode::BTN_MIDDLE,
        "side" | "back" | "x1" | "browserback" => KeyCode::BTN_SIDE,
        "extra" | "forward" | "x2" | "browserforward" => KeyCode::BTN_EXTRA,
        "back_btn" | "btn_back" => KeyCode::BTN_BACK,
        "forward_btn" | "btn_forward" => KeyCode::BTN_FORWARD,
        "task" => KeyCode::BTN_TASK,
        _ => {
            let canonical = original.to_ascii_uppercase();
            return KeyCode::from_str(&canonical)
                .map_err(|_| format!("unknown mouse button {original:?}"));
        }
    };
    Ok(code)
}

/// Parse a physical button name as it appears in the `[buttons]` table.
pub fn parse_physical_button(name: &str) -> Result<KeyCode, String> {
    let norm = normalize(name);
    match norm.as_str() {
        "left" | "primary" => Ok(KeyCode::BTN_LEFT),
        "right" | "secondary" => Ok(KeyCode::BTN_RIGHT),
        "middle" => Ok(KeyCode::BTN_MIDDLE),
        "side" | "back" => Ok(KeyCode::BTN_SIDE),
        "extra" | "forward" => Ok(KeyCode::BTN_EXTRA),
        "task" => Ok(KeyCode::BTN_TASK),
        _ => parse_mouse_button(name)
            .map_err(|_| format!("unknown physical button {name:?}"))
            .and_then(|code| {
                if is_mouse_button(code) {
                    Ok(code)
                } else {
                    Err(format!("{name:?} is not a mouse button"))
                }
            }),
    }
}

pub fn is_mouse_button(code: KeyCode) -> bool {
    (KeyCode::BTN_LEFT.code()..=KeyCode::BTN_TASK.code()).contains(&code.code())
}

/// Short, user-facing name for an evdev key code.
pub fn friendly_name(code: KeyCode) -> String {
    match code {
        KeyCode::BTN_LEFT => "left".to_string(),
        KeyCode::BTN_RIGHT => "right".to_string(),
        KeyCode::BTN_MIDDLE => "middle".to_string(),
        KeyCode::BTN_SIDE => "side".to_string(),
        KeyCode::BTN_EXTRA => "extra".to_string(),
        KeyCode::BTN_BACK => "back".to_string(),
        KeyCode::BTN_FORWARD => "forward".to_string(),
        KeyCode::BTN_TASK => "task".to_string(),
        _ => format!("{code:?}"),
    }
}

fn normalize(s: &str) -> String {
    s.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_' | '.'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_keys() {
        assert_eq!(parse_key("ctrl").unwrap(), KeyCode::KEY_LEFTCTRL);
        assert_eq!(parse_key("Page Up").unwrap(), KeyCode::KEY_PAGEUP);
        assert_eq!(parse_key("f13").unwrap(), KeyCode::KEY_F13);
        assert_eq!(parse_key("a").unwrap(), KeyCode::KEY_A);
        assert_eq!(parse_key("7").unwrap(), KeyCode::KEY_7);
        assert_eq!(parse_key("KEY_LEFTMETA").unwrap(), KeyCode::KEY_LEFTMETA);
        assert_eq!(parse_key("key_tab").unwrap(), KeyCode::KEY_TAB);
        assert!(parse_key("definitely-not-a-key").is_err());
    }

    #[test]
    fn mouse_buttons() {
        assert_eq!(parse_mouse_button("left").unwrap(), KeyCode::BTN_LEFT);
        assert_eq!(parse_mouse_button("back").unwrap(), KeyCode::BTN_SIDE);
        assert_eq!(parse_mouse_button("forward").unwrap(), KeyCode::BTN_EXTRA);
        assert_eq!(parse_physical_button("side").unwrap(), KeyCode::BTN_SIDE);
        assert!(parse_physical_button("KEY_A").is_err());
    }
}
