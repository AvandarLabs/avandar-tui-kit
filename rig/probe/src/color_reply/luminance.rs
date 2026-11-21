//! XParseColor hex-channel scaling and WCAG 2.x relative luminance math.

/// WCAG equal-contrast relative luminance cutoff separating dark (`< 0.179`) and light (`>= 0.179`) backgrounds.
pub const DARK_LUMINANCE_THRESHOLD: f64 = 0.179;

pub(super) fn scale_hex_channel(channel_hex: &str) -> u8 {
    let mut raw_value = 0_u32;
    let mut digit_count = 0_u32;
    for hex_char in channel_hex.chars() {
        if let Some(digit_value) = hex_char.to_digit(16) {
            raw_value = (raw_value << 4) | digit_value;
            digit_count += 1;
        }
    }
    if digit_count == 0 || digit_count > 4 {
        return 0;
    }
    let max_value = (1_u32 << (4 * digit_count)) - 1;
    let scaled_value = (raw_value * 255 + max_value / 2) / max_value;
    u8::try_from(scaled_value).unwrap_or(u8::MAX)
}

fn srgb_channel_to_linear(channel_value: u8) -> f64 {
    let srgb_normalized = f64::from(channel_value) / 255.0;
    if srgb_normalized <= 0.040_45 {
        srgb_normalized / 12.92
    } else {
        ((srgb_normalized + 0.055) / 1.055).powf(2.4)
    }
}

/// Computes WCAG 2.x sRGB relative luminance in `0.0..=1.0` for an 8-bit RGB triple.
#[must_use]
pub fn relative_luminance(red: u8, green: u8, blue: u8) -> f64 {
    let red_linear = srgb_channel_to_linear(red);
    let green_linear = srgb_channel_to_linear(green);
    let blue_linear = srgb_channel_to_linear(blue);
    0.2126_f64.mul_add(
        red_linear,
        0.7152_f64.mul_add(green_linear, 0.0722 * blue_linear),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wcag_relative_luminance_matches_black_white_primaries_and_dark_cutoff() {
        let black_luminance = relative_luminance(0, 0, 0);
        let white_luminance = relative_luminance(255, 255, 255);
        assert!(black_luminance.abs() < 1e-12);
        assert!((white_luminance - 1.0).abs() < 1e-12);

        assert!((relative_luminance(255, 0, 0) - 0.2126).abs() < 1e-12);
        assert!((relative_luminance(0, 255, 0) - 0.7152).abs() < 1e-12);
        assert!((relative_luminance(0, 0, 255) - 0.0722).abs() < 1e-12);

        assert!(relative_luminance(0x76, 0x76, 0x76) > DARK_LUMINANCE_THRESHOLD);
        assert!(relative_luminance(0x75, 0x75, 0x75) < DARK_LUMINANCE_THRESHOLD);
    }
}
