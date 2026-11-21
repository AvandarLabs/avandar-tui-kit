//! Unicode, portable-text, and toolkit-glyph width sample catalog for terminal probing.

/// Named character or grapheme sample measured by the terminal width probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidthSample {
    /// Unique snake-case identifier recorded in probe JSONL logs.
    pub name: &'static str,
    /// Sample group (`ascii_and_accents`, `east_asian`, `toolkit_glyph`, `emoji_consistent`, `emoji_split`, or `portable_rewrite`).
    pub category: &'static str,
    /// UTF-8 sample text written between two cursor-position queries.
    pub text: &'static str,
}

const fn sample(name: &'static str, category: &'static str, text: &'static str) -> WidthSample {
    WidthSample {
        name,
        category,
        text,
    }
}

/// Complete ordered catalog of width samples probed across every target terminal configuration.
pub const WIDTH_SAMPLES: &[WidthSample] = &[
    sample("ascii_a", "ascii_and_accents", "A"),
    sample("precomposed_acute", "ascii_and_accents", "\u{00E9}"),
    sample("combining_acute", "ascii_and_accents", "e\u{0301}"),
    sample("cjk_han", "east_asian", "\u{6F22}"),
    sample("hangul_syllable", "east_asian", "\u{D55C}"),
    sample("fullwidth_latin_a", "east_asian", "\u{FF21}"),
    sample("ellipsis", "toolkit_glyph", "\u{2026}"),
    sample("middle_dot", "toolkit_glyph", "\u{00B7}"),
    sample("chevron", "toolkit_glyph", "\u{203A}"),
    sample("pointer_triangle", "toolkit_glyph", "\u{25B8}"),
    sample("box_horizontal", "toolkit_glyph", "\u{2500}"),
    sample("box_vertical", "toolkit_glyph", "\u{2502}"),
    sample("box_top_left", "toolkit_glyph", "\u{250C}"),
    sample("box_tee_right", "toolkit_glyph", "\u{251C}"),
    sample("box_bottom_left", "toolkit_glyph", "\u{2514}"),
    sample("block_eighth_1", "toolkit_glyph", "\u{258F}"),
    sample("block_eighth_3", "toolkit_glyph", "\u{258D}"),
    sample("block_eighth_4", "toolkit_glyph", "\u{258C}"),
    sample("block_eighth_6", "toolkit_glyph", "\u{258A}"),
    sample("block_full", "toolkit_glyph", "\u{2588}"),
    sample("braille_dot_1", "toolkit_glyph", "\u{2801}"),
    sample("braille_full", "toolkit_glyph", "\u{28FF}"),
    sample("private_use_icon", "toolkit_glyph", "\u{E0B0}"),
    sample("plain_emoji", "emoji_consistent", "\u{1F600}"),
    sample(
        "family_zwj",
        "emoji_consistent",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
    ),
    sample("regional_flag_us", "emoji_consistent", "\u{1F1FA}\u{1F1F8}"),
    sample("heart_vs16", "emoji_split", "\u{2764}\u{FE0F}"),
    sample("heart_plain", "portable_rewrite", "\u{2764}"),
    sample("thumbs_up_skin_tone", "emoji_split", "\u{1F44D}\u{1F3FD}"),
    sample("thumbs_up_plain", "portable_rewrite", "\u{1F44D}"),
    sample(
        "rainbow_flag",
        "emoji_split",
        "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
    ),
    sample("white_flag_base", "portable_rewrite", "\u{1F3F3}"),
    sample("zwsp", "emoji_split", "\u{200B}"),
];

/// Computes the rendered cell width from one-based start and end cursor columns.
#[must_use]
pub const fn measured_cell_width(start_column: u16, end_column: u16) -> u16 {
    end_column.saturating_sub(start_column)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const ALLOWED_CATEGORIES: &[&str] = &[
        "ascii_and_accents",
        "east_asian",
        "toolkit_glyph",
        "emoji_consistent",
        "emoji_split",
        "portable_rewrite",
    ];

    fn find_sample(sample_name: &str) -> WidthSample {
        WIDTH_SAMPLES
            .iter()
            .copied()
            .find(|entry| entry.name == sample_name)
            .unwrap_or_else(|| panic!("missing width sample: {sample_name}"))
    }

    #[test]
    fn width_samples_have_unique_snake_case_names_and_known_categories() {
        let mut seen_names = HashSet::new();
        for entry in WIDTH_SAMPLES {
            assert!(!entry.name.is_empty() && !entry.text.is_empty());
            assert!(
                entry
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
                "expected snake_case width sample name, got {}",
                entry.name
            );
            assert!(
                ALLOWED_CATEGORIES.contains(&entry.category),
                "unexpected width sample category {} for {}",
                entry.category,
                entry.name
            );
            assert!(
                seen_names.insert(entry.name),
                "duplicate width sample name: {}",
                entry.name
            );
        }
        for required_category in ALLOWED_CATEGORIES {
            assert!(
                WIDTH_SAMPLES
                    .iter()
                    .any(|entry| entry.category == *required_category),
                "missing samples for category {required_category}"
            );
        }
    }

    #[test]
    fn portable_rewrites_strip_variation_selectors_zwj_and_skin_tone_modifiers() {
        for entry in WIDTH_SAMPLES
            .iter()
            .filter(|entry| entry.category == "portable_rewrite")
        {
            assert_eq!(
                entry.text.chars().count(),
                1,
                "portable rewrite {} must be a single base scalar",
                entry.name
            );
            assert!(
                !entry
                    .text
                    .chars()
                    .any(|character| matches!(character, '\u{FE0F}' | '\u{200D}')),
                "portable rewrite {} must omit VS16 and ZWJ",
                entry.name
            );
        }
        assert!(find_sample("heart_vs16").text.contains('\u{FE0F}'));
        assert!(find_sample("family_zwj").text.contains('\u{200D}'));
        assert_eq!(find_sample("combining_acute").text.chars().count(), 2);
    }

    #[test]
    fn toolkit_glyph_samples_are_single_bmp_scalars_without_control_bytes() {
        let toolkit_samples: Vec<_> = WIDTH_SAMPLES
            .iter()
            .filter(|entry| entry.category == "toolkit_glyph")
            .collect();
        assert_eq!(toolkit_samples.len(), 17);
        for entry in toolkit_samples {
            let mut characters = entry.text.chars();
            let scalar = characters.next().expect("non-empty glyph");
            assert_eq!(characters.next(), None, "{} must be 1 scalar", entry.name);
            assert!(!scalar.is_control());
            assert!(
                u32::from(scalar) <= 0xFFFF,
                "{} must stay in BMP",
                entry.name
            );
        }
    }

    #[test]
    fn measured_cell_width_computes_saturating_column_delta() {
        assert_eq!(measured_cell_width(1, 1), 0);
        assert_eq!(measured_cell_width(1, 2), 1);
        assert_eq!(measured_cell_width(1, 3), 2);
        assert_eq!(measured_cell_width(5, 9), 4);
        assert_eq!(measured_cell_width(4, 2), 0);
    }
}
