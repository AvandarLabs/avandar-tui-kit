//! Escape-sequence constants and pure byte builders for terminal capability probing.

/// Queries the terminal version string via `XTVERSION` (`CSI > 0 q`).
pub const XTVERSION_QUERY: &[u8] = b"\x1b[>0q";

/// Primary device attributes query (`CSI c`), used as an in-band reply fence.
pub const DA1_FENCE_QUERY: &[u8] = b"\x1b[c";

/// Queries the active Kitty keyboard enhancement flags (`CSI ? u`).
pub const KITTY_KEYBOARD_QUERY: &[u8] = b"\x1b[?u";

/// Pushes Kitty keyboard disambiguate-escape-codes flag (`1`) onto the stack (`CSI > 1 u`).
pub const KITTY_KEYBOARD_PUSH_DISAMBIGUATE: &[u8] = b"\x1b[>1u";

/// Pops one entry off the Kitty keyboard enhancement stack (`CSI < u`).
pub const KITTY_KEYBOARD_POP: &[u8] = b"\x1b[<u";

/// Queries cell dimensions in pixels (`CSI 16 t`).
pub const CELL_SIZE_PIXELS_QUERY: &[u8] = b"\x1b[16t";

/// Queries cursor position via `DSR` (`CSI 6 n`).
pub const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";

/// Queries the terminal default background color via `OSC 11` (`OSC 11 ; ? BEL`).
pub const OSC_11_BACKGROUND_QUERY: &[u8] = b"\x1b]11;?\x07";

/// Enables button-event mouse tracking (`1000`, `1002`) with SGR encoding (`1006`) per rules M1 and M2.
pub const MOUSE_SETUP_M1: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

/// Resets every standard and extended mouse tracking and encoding mode per rule M5.
pub const MOUSE_TEARDOWN_M5: &[u8] =
    b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";

/// Enables bracketed paste mode (`CSI ? 2004 h`).
pub const BRACKETED_PASTE_ON: &[u8] = b"\x1b[?2004h";

/// Disables bracketed paste mode (`CSI ? 2004 l`).
pub const BRACKETED_PASTE_OFF: &[u8] = b"\x1b[?2004l";

/// Enables terminal focus-in / focus-out reporting (`CSI ? 1004 h`).
pub const FOCUS_EVENTS_ON: &[u8] = b"\x1b[?1004h";

/// Disables terminal focus-in / focus-out reporting (`CSI ? 1004 l`).
pub const FOCUS_EVENTS_OFF: &[u8] = b"\x1b[?1004l";

/// Begins a synchronized output batch (`CSI ? 2026 h`).
pub const SYNC_OUTPUT_ON: &[u8] = b"\x1b[?2026h";

/// Ends a synchronized output batch (`CSI ? 2026 l`).
pub const SYNC_OUTPUT_OFF: &[u8] = b"\x1b[?2026l";

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes `input` using the RFC 4648 standard Base64 alphabet with `=` padding.
#[must_use]
pub fn encode_base64(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let byte0 = usize::from(chunk[0]);
        let byte1 = chunk.get(1).copied().map_or(0, usize::from);
        let byte2 = chunk.get(2).copied().map_or(0, usize::from);
        let triple = (byte0 << 16) | (byte1 << 8) | byte2;

        output.push(char::from(BASE64_ALPHABET[(triple >> 18) & 0x3f]));
        output.push(char::from(BASE64_ALPHABET[(triple >> 12) & 0x3f]));
        if chunk.len() > 1 {
            output.push(char::from(BASE64_ALPHABET[(triple >> 6) & 0x3f]));
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(char::from(BASE64_ALPHABET[triple & 0x3f]));
        } else {
            output.push('=');
        }
    }
    output
}

/// Builds a `DECRPM` private mode status query (`CSI ? <mode> $ p`).
#[must_use]
pub fn build_decrpm_query(mode: u16) -> Vec<u8> {
    format!("\x1b[?{mode}$p").into_bytes()
}

/// Builds a direct `OSC 52` clipboard write sequence terminated by `BEL`.
#[must_use]
pub fn build_osc52_plain(payload: &[u8]) -> Vec<u8> {
    let encoded = encode_base64(payload);
    let mut sequence = Vec::with_capacity(7 + encoded.len());
    sequence.extend_from_slice(b"\x1b]52;c;");
    sequence.extend_from_slice(encoded.as_bytes());
    sequence.push(b'\x07');
    sequence
}

/// Builds a `tmux` DCS passthrough-wrapped `OSC 52` clipboard write sequence.
///
/// Every `ESC` byte inside the inner `OSC 52` sequence is doubled as required by
/// `tmux` passthrough framing (`DCS tmux; ... ST`).
#[must_use]
pub fn build_osc52_tmux(payload: &[u8]) -> Vec<u8> {
    let inner = build_osc52_plain(payload);
    let mut sequence = Vec::with_capacity(inner.len() + 10);
    sequence.extend_from_slice(b"\x1bPtmux;");
    for &byte in &inner {
        if byte == b'\x1b' {
            sequence.push(b'\x1b');
        }
        sequence.push(byte);
    }
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_setup_and_teardown_follow_rules_m1_m2_and_m5() {
        assert_eq!(MOUSE_SETUP_M1, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
        assert!(!MOUSE_SETUP_M1.windows(5).any(|window| window == b"1003h"));
        assert_eq!(
            MOUSE_TEARDOWN_M5,
            b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l"
        );
    }

    #[test]
    fn builds_decrpm_and_osc52_sequences_with_padding() {
        assert_eq!(build_decrpm_query(1002), b"\x1b[?1002$p");
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64("漢".as_bytes()), "5ryi");
        assert_eq!(build_osc52_plain(b"probe-1"), b"\x1b]52;c;cHJvYmUtMQ==\x07");
        assert_eq!(
            build_osc52_tmux(b"probe-1"),
            b"\x1bPtmux;\x1b\x1b]52;c;cHJvYmUtMQ==\x07\x1b\\"
        );
    }

    #[test]
    fn query_and_mode_toggle_constants_match_exact_wire_bytes() {
        assert_eq!(XTVERSION_QUERY, b"\x1b[>0q");
        assert_eq!(DA1_FENCE_QUERY, b"\x1b[c");
        assert_eq!(KITTY_KEYBOARD_QUERY, b"\x1b[?u");
        assert_eq!(KITTY_KEYBOARD_PUSH_DISAMBIGUATE, b"\x1b[>1u");
        assert_eq!(KITTY_KEYBOARD_POP, b"\x1b[<u");
        assert_eq!(CELL_SIZE_PIXELS_QUERY, b"\x1b[16t");
        assert_eq!(CURSOR_POSITION_QUERY, b"\x1b[6n");
        assert_eq!(OSC_11_BACKGROUND_QUERY, b"\x1b]11;?\x07");
        assert_eq!(BRACKETED_PASTE_ON, b"\x1b[?2004h");
        assert_eq!(BRACKETED_PASTE_OFF, b"\x1b[?2004l");
        assert_eq!(FOCUS_EVENTS_ON, b"\x1b[?1004h");
        assert_eq!(FOCUS_EVENTS_OFF, b"\x1b[?1004l");
        assert_eq!(SYNC_OUTPUT_ON, b"\x1b[?2026h");
        assert_eq!(SYNC_OUTPUT_OFF, b"\x1b[?2026l");
    }
}
