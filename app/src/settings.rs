//! File › Settings, after Google Sheets' "Settings for this spreadsheet": the locale (how typed
//! dates are read, e.g. 05/08/2026 as 5 August in the UK), the default date, time and number
//! formats new entries get, and the display language of GridCraft's dialogs. Kept in the browser,
//! and handed to GridCraft's input reader (patch 0026).

use egui::{Color32, RichText, vec2};
use gridcraft_engine::core::Value;
use gridcraft_ui_egui::SheetApp;
use serde::{Deserialize, Serialize};

use crate::theme;

/// Asks for the Settings window (File › Settings), through egui's memory.
pub const REQUEST: &str = "starsheets_settings";
const STORE_KEY: &str = "starsheets_settings";

/// A locale: name, day first?, date, time and date-time formats.
const LOCALES: &[(&str, bool, &str, &str, &str)] = &[
    ("English (United Kingdom)", true, "dd/mm/yyyy", "hh:mm:ss", "dd/mm/yyyy hh:mm:ss"),
    ("English (United States)", false, "m/d/yyyy", "h:mm:ss AM/PM", "m/d/yyyy h:mm:ss"),
    ("English (Ireland)", true, "dd/mm/yyyy", "hh:mm:ss", "dd/mm/yyyy hh:mm:ss"),
    ("English (Australia)", true, "d/mm/yyyy", "h:mm:ss AM/PM", "d/mm/yyyy h:mm:ss"),
    ("English (Canada)", false, "yyyy-mm-dd", "h:mm:ss AM/PM", "yyyy-mm-dd h:mm:ss"),
    ("Deutsch (Deutschland)", true, "dd.mm.yyyy", "hh:mm:ss", "dd.mm.yyyy hh:mm:ss"),
    ("Español (España)", true, "d/m/yyyy", "hh:mm:ss", "d/m/yyyy hh:mm:ss"),
    ("Français (France)", true, "dd/mm/yyyy", "hh:mm:ss", "dd/mm/yyyy hh:mm:ss"),
    ("Italiano (Italia)", true, "dd/mm/yyyy", "hh:mm:ss", "dd/mm/yyyy hh:mm:ss"),
    ("Nederlands (Nederland)", true, "d-m-yyyy", "hh:mm:ss", "d-m-yyyy hh:mm:ss"),
    ("Português (Brasil)", true, "dd/mm/yyyy", "hh:mm:ss", "dd/mm/yyyy hh:mm:ss"),
    ("日本語 (日本)", false, "yyyy/mm/dd", "hh:mm:ss", "yyyy/mm/dd hh:mm:ss"),
];

const DATE_FORMATS: &[&str] = &["dd/mm/yyyy", "d/m/yyyy", "dd-mm-yyyy", "dd.mm.yyyy", "m/d/yyyy", "mm/dd/yyyy", "yyyy-mm-dd", "d mmm yyyy", "d mmmm yyyy", "dd-mmm-yyyy", "dddd, d mmmm yyyy"];
const TIME_FORMATS: &[&str] = &["hh:mm:ss", "hh:mm", "h:mm:ss AM/PM", "h:mm AM/PM"];
/// Default number formats ("" = Automatic, as typed).
const NUMBER_FORMATS: &[(&str, &str)] = &[("Automatic", ""), ("1,000.12", "#,##0.00"), ("1000.12", "0.00"), ("1,000", "#,##0"), ("1000", "0")];
/// The example shown for each choice: 26 September 2008, 15:59:00 (Google's sample).
const SAMPLE: f64 = 39_717.666_e0;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Prefs {
    pub locale: usize,
    pub date: String,
    pub time: String,
    pub number: String,
    pub language: usize,
}

impl Default for Prefs {
    fn default() -> Prefs {
        let (_, _, date, time, _) = LOCALES[0];
        Prefs { locale: 0, date: date.into(), time: time.into(), number: String::new(), language: 0 }
    }
}

impl Prefs {
    pub fn load() -> Prefs {
        #[cfg(target_arch = "wasm32")]
        if let Some(p) = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item(STORE_KEY).ok().flatten())
            .and_then(|t| serde_json::from_str::<Prefs>(&t).ok())
        {
            return p;
        }
        let _ = STORE_KEY;
        Prefs::default()
    }

    fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        if let (Some(s), Ok(t)) = (web_sys::window().and_then(|w| w.local_storage().ok().flatten()), serde_json::to_string(self)) {
            let _ = s.set_item(STORE_KEY, &t);
        }
    }

    /// Hands the locale to GridCraft's input reader, and the language to its dialogs.
    pub fn apply(&self, app: &mut SheetApp) {
        let (_, day_first, ..) = LOCALES[self.locale.min(LOCALES.len() - 1)];
        let datetime = format!("{} {}", self.date, self.time);
        let number = (!self.number.is_empty()).then_some(self.number.as_str());
        gridcraft_engine::core::parse::set_input_locale(day_first, Some(&self.date), Some(&self.time), Some(&datetime), number);
        if let Some(lang) = gridcraft_ui_egui::i18n::Language::ALL.get(self.language) {
            app.ui.language = *lang;
        }
        DEFAULTS.with(|d| *d.borrow_mut() = (self.date.clone(), self.time.clone()));
    }
}

thread_local! {
    /// The default date and time formats, for Format › Number › Date / Time / Date time.
    static DEFAULTS: std::cell::RefCell<(String, String)> = std::cell::RefCell::new(("dd/mm/yyyy".into(), "hh:mm:ss".into()));
}

/// The default date, time and date-time format codes (Format › Number uses them).
pub fn default_codes() -> (String, String, String) {
    DEFAULTS.with(|d| {
        let (date, time) = d.borrow().clone();
        let dt = format!("{date} {time}");
        (date, time, dt)
    })
}

fn sample(code: &str) -> String {
    if code.is_empty() {
        return "1000.12".into();
    }
    let v = Value::number(if code.contains('#') || code.starts_with('0') { 1000.12 } else { SAMPLE });
    gridcraft_engine::numfmt::text_function(&v, code, gridcraft_engine::core::date::DateSystem::default()).unwrap_or_else(|_| code.to_string())
}

#[derive(Default)]
pub struct SettingsWindow {
    draft: Option<Prefs>,
}

impl SettingsWindow {
    pub fn ui(&mut self, app: &mut SheetApp, ctx: &egui::Context) {
        if ctx.data_mut(|m| m.remove_temp::<bool>(egui::Id::new(REQUEST))).is_some() {
            self.draft = Some(Prefs::load());
        }
        let Some(d) = self.draft.as_mut() else { return };
        let mut done = None;
        egui::Modal::new(egui::Id::new("starsheets_settings")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new("Settings").size(20.0).color(theme::TEXT));
            ui.add_space(4.0);
            ui.label(RichText::new("These apply to everything you open in this browser.").color(theme::TEXT_DIM));
            ui.add_space(14.0);
            let row = |ui: &mut egui::Ui, label: &str, note: &str| {
                ui.label(RichText::new(label).size(14.0).strong().color(theme::TEXT));
                if !note.is_empty() {
                    ui.label(RichText::new(note).size(12.0).color(theme::TEXT_DIM));
                }
            };
            row(ui, "Locale", "How typed dates are read (05/08/2026 is 5 August in the UK, May 8 in the US), and the formats below.");
            let before = d.locale;
            egui::ComboBox::from_id_salt("set_locale").width(500.0).selected_text(LOCALES[d.locale].0).show_ui(ui, |ui| {
                for (i, l) in LOCALES.iter().enumerate() {
                    ui.selectable_value(&mut d.locale, i, l.0);
                }
            });
            if d.locale != before {
                let (_, _, date, time, _) = LOCALES[d.locale];
                d.date = date.into();
                d.time = time.into();
            }
            ui.add_space(12.0);
            ui.columns(3, |cols| {
                row(&mut cols[0], "Date", "");
                egui::ComboBox::from_id_salt("set_date").width(150.0).selected_text(sample(&d.date)).show_ui(&mut cols[0], |ui| {
                    for f in DATE_FORMATS {
                        ui.selectable_value(&mut d.date, f.to_string(), sample(f));
                    }
                });
                row(&mut cols[1], "Time", "");
                egui::ComboBox::from_id_salt("set_time").width(150.0).selected_text(sample(&d.time)).show_ui(&mut cols[1], |ui| {
                    for f in TIME_FORMATS {
                        ui.selectable_value(&mut d.time, f.to_string(), sample(f));
                    }
                });
                row(&mut cols[2], "Numbers", "");
                let shown = NUMBER_FORMATS.iter().find(|(_, c)| *c == d.number).map_or("Automatic", |(l, _)| l);
                egui::ComboBox::from_id_salt("set_number").width(150.0).selected_text(shown).show_ui(&mut cols[2], |ui| {
                    for (label, code) in NUMBER_FORMATS {
                        ui.selectable_value(&mut d.number, code.to_string(), *label);
                    }
                });
            });
            ui.label(RichText::new("New dates, times and numbers you type get these formats; cells already formatted keep theirs.").size(12.0).color(theme::TEXT_DIM));
            ui.add_space(12.0);
            row(ui, "Display language", "For dialogs. StarSheets' own menus and toolbar are in English for now.");
            let langs = gridcraft_ui_egui::i18n::Language::ALL;
            egui::ComboBox::from_id_salt("set_lang").width(500.0).selected_text(langs.get(d.language).map_or("English", |l| l.name())).show_ui(ui, |ui| {
                for (i, l) in langs.iter().enumerate() {
                    ui.selectable_value(&mut d.language, i, l.name());
                }
            });
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.add_space(520.0 - 210.0);
                let green = Color32::from_rgb(0x14, 0x6C, 0x2E);
                if ui.add(egui::Button::new(RichText::new("Cancel").color(green)).fill(Color32::WHITE).stroke(egui::Stroke::new(1.0, theme::BOX)).corner_radius(4.0).min_size(vec2(96.0, 36.0))).clicked() {
                    done = Some(false);
                }
                if ui.add(egui::Button::new(RichText::new("Save settings").color(Color32::WHITE)).fill(green).corner_radius(4.0).min_size(vec2(110.0, 36.0))).clicked() {
                    done = Some(true);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(false);
            }
        });
        match done {
            Some(true) => {
                if let Some(p) = self.draft.take() {
                    p.save();
                    p.apply(app);
                }
            }
            Some(false) => self.draft = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use gridcraft_engine::core::date::DateSystem;
    use gridcraft_engine::core::parse::{parse_input, set_input_locale};

    #[test]
    fn a_day_first_locale_reads_dates_day_first() {
        set_input_locale(true, Some("dd/mm/yyyy"), Some("hh:mm:ss"), Some("dd/mm/yyyy hh:mm:ss"), None);
        let p = parse_input("05/08/1930", DateSystem::default());
        // 5 August 1930 is serial 11175 (8 May would be 11086).
        assert_eq!(p.value.as_f64(), Some(11_175.0));
        assert_eq!(p.format, Some("dd/mm/yyyy"));
        let t = parse_input("15:59", DateSystem::default());
        assert_eq!(t.format, Some("hh:mm:ss"));
        set_input_locale(false, None, None, None, None);
        assert_eq!(parse_input("05/08/1930", DateSystem::default()).value.as_f64(), Some(11_086.0));
    }
}
