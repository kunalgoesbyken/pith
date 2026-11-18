use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

use crate::registry;

pub const TYPING_SETTINGS_PAGE: &str = "ms-settings:typing";
const INPUT_SETTINGS_KEY: &str = r"Software\Microsoft\Input\Settings";
const HARDWARE_KEYBOARD_SUGGESTIONS: &str = "EnableHwkbTextPrediction";
const SUGGESTIONS_ON: u32 = 1;

const PREDICTORS: [(&str, &str); 3] = [
    ("Grammarly.Desktop", "Grammarly"),
    ("PhraseExpress", "PhraseExpress"),
    ("Lightkey", "Lightkey"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rival {
    TextSuggestions,
    App(&'static str),
}

impl Rival {
    pub fn settings_page(&self) -> Option<&'static str> {
        matches!(self, Self::TextSuggestions).then_some(TYPING_SETTINGS_PAGE)
    }
}

fn running_images() -> Vec<String> {
    let mut images = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return images;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len());
            images.push(String::from_utf16_lossy(&entry.szExeFile[..length]));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    images
}

fn find_predictor(table: &[(&'static str, &'static str)], images: &[String]) -> Option<Rival> {
    let running: Vec<String> = images
        .iter()
        .map(|image| pith_core::app_name(image))
        .collect();
    table
        .iter()
        .find(|(stem, _)| running.iter().any(|name| name.eq_ignore_ascii_case(stem)))
        .map(|(_, shown)| Rival::App(shown))
}

pub fn detect() -> Option<Rival> {
    let os_suggestions = registry::read_dword(INPUT_SETTINGS_KEY, HARDWARE_KEYBOARD_SUGGESTIONS)
        == Some(SUGGESTIONS_ON);
    if os_suggestions {
        return Some(Rival::TextSuggestions);
    }
    find_predictor(&PREDICTORS, &running_images())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_running_predictor_is_matched_by_image_name_ignoring_case_and_extension() {
        for name in ["Lightkey.exe", "LIGHTKEY.EXE", r"C:\Apps\lightkey.exe"] {
            assert_eq!(
                find_predictor(&PREDICTORS, &images(&["explorer.exe", name])),
                Some(Rival::App("Lightkey")),
                "{name}"
            );
        }
    }

    #[test]
    fn unrelated_and_look_alike_processes_are_not_predictors() {
        let running = images(&["explorer.exe", "lightkeyboard.exe", "Grammarly.exe", ""]);
        assert_eq!(find_predictor(&PREDICTORS, &running), None);
        assert_eq!(find_predictor(&PREDICTORS, &[]), None);
    }

    #[test]
    fn the_first_table_entry_wins_when_several_are_running() {
        let running = images(&["lightkey.exe", "Grammarly.Desktop.exe"]);
        assert_eq!(
            find_predictor(&PREDICTORS, &running),
            Some(Rival::App("Grammarly"))
        );
    }

    #[test]
    fn only_the_operating_system_feature_offers_a_settings_page() {
        assert_eq!(
            Rival::TextSuggestions.settings_page(),
            Some("ms-settings:typing")
        );
        assert_eq!(Rival::App("Lightkey").settings_page(), None);
    }

    #[test]
    fn the_process_list_includes_this_test_process() {
        let own = std::env::current_exe().unwrap();
        let own = pith_core::app_name(&own.to_string_lossy());
        assert!(
            running_images()
                .iter()
                .any(|image| pith_core::app_name(image).eq_ignore_ascii_case(&own))
        );
    }
}
