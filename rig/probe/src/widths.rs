//! Unicode, portable-text, and toolkit-glyph width sample catalog for Milestone 0.

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

    fn find_sample(sample_name: &str) -> WidthSample {
        WIDTH_SAMPLES
            .iter()
            .copied()
            .find(|entry| entry.name == sample_name)
            .unwrap_or_else(|| panic!("missing width sample: {sample_name}"))
    }

    #[test]
    fn catalog_has_unique_non_empty_names_categories_and_texts() {
        assert_eq!(WIDTH_SAMPLES.len(), 33);
        let mut seen_names = HashSet::new();
        for entry in WIDTH_SAMPLES {
            assert!(!entry.name.is_empty());
            assert!(!entry.category.is_empty());
            assert!(!entry.text.is_empty());
            assert!(
                seen_names.insert(entry.name),
                "duplicate width sample name: {}",
                entry.name
            );
        }
    }

    #[test]
    fn catalog_covers_ascii_east_asian_and_emoji_samples_including_m0_row_19() {
        let expected_samples = [
            ("ascii_a", "ascii_and_accents", "A"),
            ("precomposed_acute", "ascii_and_accents", "\u{00E9}"),
            ("combining_acute", "ascii_and_accents", "e\u{0301}"),
            ("cjk_han", "east_asian", "\u{6F22}"),
            ("hangul_syllable", "east_asian", "\u{D55C}"),
            ("fullwidth_latin_a", "east_asian", "\u{FF21}"),
            ("plain_emoji", "emoji_consistent", "\u{1F600}"),
            (
                "family_zwj",
                "emoji_consistent",
                "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            ),
            ("regional_flag_us", "emoji_consistent", "\u{1F1FA}\u{1F1F8}"),
            ("heart_vs16", "emoji_split", "\u{2764}\u{FE0F}"),
            ("heart_plain", "portable_rewrite", "\u{2764}"),
            ("thumbs_up_skin_tone", "emoji_split", "\u{1F44D}\u{1F3FD}"),
            ("thumbs_up_plain", "portable_rewrite", "\u{1F44D}"),
            (
                "rainbow_flag",
                "emoji_split",
                "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
            ),
            ("white_flag_base", "portable_rewrite", "\u{1F3F3}"),
            ("zwsp", "emoji_split", "\u{200B}"),
        ];
        for (expected_name, expected_category, expected_text) in expected_samples {
            let found_sample = find_sample(expected_name);
            assert_eq!(found_sample.category, expected_category);
            assert_eq!(found_sample.text, expected_text);
        }
    }

    #[test]
    fn catalog_covers_all_m0_row_20_toolkit_glyphs_and_private_use_icon() {
        let expected_glyphs = [
            ("ellipsis", "\u{2026}"),
            ("middle_dot", "\u{00B7}"),
            ("chevron", "\u{203A}"),
            ("pointer_triangle", "\u{25B8}"),
            ("box_horizontal", "\u{2500}"),
            ("box_vertical", "\u{2502}"),
            ("box_top_left", "\u{250C}"),
            ("box_tee_right", "\u{251C}"),
            ("box_bottom_left", "\u{2514}"),
            ("block_eighth_1", "\u{258F}"),
            ("block_eighth_3", "\u{258D}"),
            ("block_eighth_4", "\u{258C}"),
            ("block_eighth_6", "\u{258A}"),
            ("block_full", "\u{2588}"),
            ("braille_dot_1", "\u{2801}"),
            ("braille_full", "\u{28FF}"),
            ("private_use_icon", "\u{E0B0}"),
        ];
        for (expected_name, expected_text) in expected_glyphs {
            let found_sample = find_sample(expected_name);
            assert_eq!(found_sample.category, "toolkit_glyph");
            assert_eq!(found_sample.text, expected_text);
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
