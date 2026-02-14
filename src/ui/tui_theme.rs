use eframe::egui;
use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, Visuals, Vec2};

// ═══════════════════════════════════════════════════════
//  PALETA DE COLORES — Terminal Moderno
// ═══════════════════════════════════════════════════════

pub const BG_PRIMARY:   Color32 = Color32::from_rgb(0x0d, 0x11, 0x17); // Fondo principal
pub const BG_SECONDARY: Color32 = Color32::from_rgb(0x16, 0x1b, 0x22); // Paneles secundarios
pub const BG_HOVER:     Color32 = Color32::from_rgb(0x21, 0x26, 0x2d); // Hover
pub const BG_ACTIVE:    Color32 = Color32::from_rgb(0x30, 0x36, 0x3d); // Active/Selected

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xc9, 0xd1, 0xd9); // Texto principal
pub const TEXT_DIM:     Color32 = Color32::from_rgb(0x6e, 0x76, 0x81); // Texto secundario
pub const ACCENT:       Color32 = Color32::from_rgb(0x00, 0xd4, 0xff); // Azul moderno
pub const ACCENT_DIM:   Color32 = Color32::from_rgb(0x00, 0x8a, 0xb3); // Azul oscuro
pub const SUCCESS:      Color32 = Color32::from_rgb(0x3f, 0xb9, 0x50); // Verde (status)
pub const ERROR:        Color32 = Color32::from_rgb(0xf8, 0x51, 0x49); // Rojo (status)
pub const WARNING:      Color32 = Color32::from_rgb(0xd2, 0x9a, 0x22); // Amarillo/naranja
pub const BORDER:       Color32 = Color32::from_rgb(0x30, 0x36, 0x3d); // Bordes

// Neon UI — para romper monotonía
pub const NEON_YELLOW:  Color32 = Color32::from_rgb(0xff, 0xf0, 0x3f); // Descargas/carga
pub const NEON_GREEN:   Color32 = Color32::from_rgb(0x39, 0xff, 0x14); // Confirmar/guardar
pub const NEON_RED:     Color32 = Color32::from_rgb(0xff, 0x2e, 0x4c); // Cancelar/borrar
pub const NEON_PINK:    Color32 = Color32::from_rgb(0xff, 0x6e, 0xff); // Números destacados

// ═══════════════════════════════════════════════════════
//  APPLY THEME
// ═══════════════════════════════════════════════════════

pub fn apply_tui_theme(ctx: &egui::Context) {
    // --- Fuente Monoespaciada Universal ---
    let mut fonts = egui::FontDefinitions::default();
    // Reasignar Proportional para que use Monospace
    fonts.families.insert(
        FontFamily::Proportional,
        fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default(),
    );
    ctx.set_fonts(fonts);

    // --- Estilos ---
    let mut style = (*ctx.style()).clone();

    // Tamaños de texto
    style.text_styles.insert(egui::TextStyle::Body,    FontId::new(13.0, FontFamily::Monospace));
    style.text_styles.insert(egui::TextStyle::Button,  FontId::new(13.0, FontFamily::Monospace));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::new(14.0, FontFamily::Monospace));
    style.text_styles.insert(egui::TextStyle::Small,   FontId::new(11.0, FontFamily::Monospace));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));

    // Spacing
    style.spacing.item_spacing = Vec2::new(6.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 2.0);
    style.spacing.window_margin = egui::Margin::same(8);

    ctx.set_style(style);

    // --- Visuals ---
    let mut visuals = Visuals::dark();

    // Fondos
    visuals.panel_fill = BG_PRIMARY;
    visuals.window_fill = BG_SECONDARY;
    visuals.extreme_bg_color = Color32::from_rgb(0x05, 0x08, 0x0c);
    visuals.faint_bg_color = BG_SECONDARY;

    // Zero rounding on all widgets
    visuals.widgets.inactive.corner_radius = CornerRadius::ZERO;
    visuals.widgets.inactive.bg_fill = BG_SECONDARY;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.weak_bg_fill = BG_SECONDARY;

    visuals.widgets.hovered.corner_radius = CornerRadius::ZERO;
    visuals.widgets.hovered.bg_fill = BG_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.weak_bg_fill = BG_HOVER;

    visuals.widgets.active.corner_radius = CornerRadius::ZERO;
    visuals.widgets.active.bg_fill = BG_ACTIVE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    visuals.widgets.active.weak_bg_fill = BG_ACTIVE;

    visuals.widgets.open.corner_radius = CornerRadius::ZERO;
    visuals.widgets.open.bg_fill = BG_ACTIVE;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.open.weak_bg_fill = BG_ACTIVE;

    visuals.widgets.noninteractive.corner_radius = CornerRadius::ZERO;
    visuals.widgets.noninteractive.bg_fill = BG_PRIMARY;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.weak_bg_fill = BG_PRIMARY;

    // Selection — fondo sólido oscuro para que el texto se lea bien
    visuals.selection.bg_fill = Color32::from_rgb(0x0a, 0x3d, 0x5c); // Azul oscuro sólido
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_DIM);

    // Text colors
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.window_stroke = Stroke::new(1.0, BORDER);

    // Separators
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    ctx.set_visuals(visuals);
}

// ═══════════════════════════════════════════════════════
//  WIDGET HELPERS
// ═══════════════════════════════════════════════════════

/// Botón estilo TUI: `[TEXTO]`
pub fn tui_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(format!("[{}]", label))
        .family(FontFamily::Monospace)
        .color(ACCENT);
    ui.add(egui::Button::new(text)
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO))
}

/// Botón TUI con color custom: `[TEXTO]`
pub fn tui_button_c(ui: &mut egui::Ui, label: &str, color: Color32) -> egui::Response {
    let text = egui::RichText::new(format!("[{}]", label))
        .family(FontFamily::Monospace)
        .color(color);
    ui.add(egui::Button::new(text)
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO))
}

/// Número destacado en neon pink
pub fn tui_number(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Monospace)
            .color(NEON_PINK)
    );
}

/// Checkbox estilo TUI: `[x]` / `[ ]`
pub fn tui_checkbox(ui: &mut egui::Ui, checked: &mut bool) -> egui::Response {
    let label = if *checked { "[x]" } else { "[ ]" };
    let text = egui::RichText::new(label)
        .family(FontFamily::Monospace)
        .color(if *checked { ACCENT } else { TEXT_DIM });
    let resp = ui.add(egui::Button::new(text)
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO));
    if resp.clicked() {
        *checked = !*checked;
    }
    resp
}

/// Heading estilo TUI: `═══ TEXTO ═══`
pub fn tui_heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(format!("═══ {} ═══", text))
            .family(FontFamily::Monospace)
            .color(ACCENT)
            .strong()
    );
}

/// Separador estilo TUI: `────────────────`
pub fn tui_separator(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let chars = (width / 7.5) as usize; // ~7.5px per monospace char at 13px
    ui.label(
        egui::RichText::new("─".repeat(chars.max(4)))
            .family(FontFamily::Monospace)
            .color(BORDER)
            .size(11.0)
    );
}

/// Label de status TUI con color
pub fn tui_status(ui: &mut egui::Ui, status: &str, color: Color32) {
    ui.label(
        egui::RichText::new(status)
            .family(FontFamily::Monospace)
            .color(color)
    );
}

/// Tab estilo TUI: `[ TEXTO ]` seleccionado vs `  TEXTO  `
pub fn tui_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = if selected {
        egui::RichText::new(format!("[ {} ]", label))
            .family(FontFamily::Monospace)
            .color(ACCENT)
            .strong()
    } else {
        egui::RichText::new(format!("  {}  ", label))
            .family(FontFamily::Monospace)
            .color(TEXT_DIM)
    };
    ui.add(egui::Button::new(text)
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO))
}

/// Texto con color dim para info secundaria
pub fn tui_dim(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Monospace)
            .color(TEXT_DIM)
            .size(11.0)
    );
}

/// Texto accent para nombres/títulos
pub fn tui_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .family(FontFamily::Monospace)
            .color(TEXT_PRIMARY)
    );
}
