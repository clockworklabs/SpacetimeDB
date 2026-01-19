//! SpacetimeDB brand colors for the TUI.

use ratatui::style::Color;

/// SpacetimeDB brand color palette.
pub mod brand {
    use super::Color;

    // Primary colors
    pub const GREEN: Color = Color::Rgb(0x4c, 0xf4, 0x90);        // #4cf490
    pub const PURPLE: Color = Color::Rgb(0xa8, 0x80, 0xff);       // #a880ff
    pub const PURPLE_2: Color = Color::Rgb(0x8a, 0x38, 0xf5);     // #8a38f5
    pub const YELLOW: Color = Color::Rgb(0xfb, 0xdc, 0x8e);       // #fbdc8e
    pub const BLUE: Color = Color::Rgb(0x02, 0xbe, 0xfa);         // #02befa
    pub const PINK: Color = Color::Rgb(0xff, 0x80, 0xfb);         // #ff80fb
    pub const TEAL: Color = Color::Rgb(0x00, 0xcc, 0xb4);         // #00ccb4
    pub const ORANGE: Color = Color::Rgb(0xf6, 0x9d, 0x50);       // #f69d50
    pub const RED: Color = Color::Rgb(0xff, 0x4c, 0x4c);          // #ff4c4c
    pub const RED_2: Color = Color::Rgb(0xfc, 0x68, 0x97);        // #fc6897

    // Background shades
    pub const SHADE_1: Color = Color::Rgb(0x14, 0x14, 0x16);      // #141416
    pub const SHADE_2: Color = Color::Rgb(0x0d, 0x0d, 0x0e);      // #0d0d0e (main bg)
    pub const BG_DARK: Color = Color::Rgb(0x06, 0x06, 0x06);      // #060606

    // Grayscale
    pub const N1: Color = Color::Rgb(0xe6, 0xe9, 0xf0);           // #e6e9f0 (main text)
    pub const N2: Color = Color::Rgb(0xce, 0xd3, 0xe0);           // #ced3e0
    pub const N3: Color = Color::Rgb(0xb6, 0xc0, 0xcf);           // #b6c0cf
    pub const N4: Color = Color::Rgb(0x6f, 0x79, 0x87);           // #6f7987 (dimmed)
    pub const N5: Color = Color::Rgb(0x36, 0x38, 0x40);           // #363840
    pub const N6: Color = Color::Rgb(0x20, 0x21, 0x26);           // #202126 (borders)
}

/// Semantic color aliases for UI elements.
pub mod ui {
    use super::brand;
    use super::Color;

    // Text colors
    pub const TEXT: Color = brand::N1;
    pub const TEXT_DIMMED: Color = brand::N4;
    pub const TEXT_MUTED: Color = brand::N3;

    // Accent colors
    pub const ACCENT: Color = brand::GREEN;
    pub const ACCENT_SECONDARY: Color = brand::PURPLE;
    pub const ACCENT_INFO: Color = brand::BLUE;

    // Status colors
    pub const SUCCESS: Color = brand::GREEN;
    pub const WARNING: Color = brand::YELLOW;
    pub const ERROR: Color = brand::RED;
    pub const INFO: Color = brand::BLUE;

    // Background colors
    pub const BG: Color = brand::SHADE_2;
    pub const BG_HEADER: Color = brand::BG_DARK;
    pub const BG_PANEL: Color = brand::SHADE_1;
    pub const BG_SELECTED: Color = brand::N5;
    pub const BG_HOVER: Color = brand::N6;

    // Border colors
    pub const BORDER: Color = brand::N6;
    pub const BORDER_FOCUSED: Color = brand::GREEN;
    pub const BORDER_DIMMED: Color = brand::N5;

    // Log level colors
    pub const LOG_ERROR: Color = brand::RED;
    pub const LOG_WARN: Color = brand::YELLOW;
    pub const LOG_INFO: Color = brand::BLUE;
    pub const LOG_DEBUG: Color = brand::N3;
    pub const LOG_TRACE: Color = brand::N4;
    pub const LOG_PANIC: Color = brand::RED_2;

    // Chat colors
    pub const CHAT_USER: Color = brand::GREEN;
    pub const CHAT_ASSISTANT: Color = brand::PURPLE;
    pub const CHAT_SYSTEM: Color = brand::YELLOW;

    // Code colors
    pub const CODE_KEYWORD: Color = brand::PURPLE;
    pub const CODE_STRING: Color = brand::GREEN;
    pub const CODE_COMMENT: Color = brand::N4;
    pub const CODE_FUNCTION: Color = brand::BLUE;

    // Diff colors
    pub const DIFF_ADD: Color = brand::GREEN;
    pub const DIFF_REMOVE: Color = brand::RED;
    pub const DIFF_CONTEXT: Color = brand::N3;
    pub const DIFF_HEADER: Color = brand::PURPLE;
}
