use pith_core::{Access, Config, Counts, ForegroundApp, ModelPreset, Pause, Setting};
use pith_windows::{Overview, Page, Rival, SettingsWindow, StatsView};

use crate::capture::{capture, park, resize};

pub(crate) const FULL_PAGE: (i32, i32) = (880, 1000);
const SMALL_LAPTOP_CLIENT: (i32, i32) = (800, 392);
const PAGE_NAMES: [&str; 6] = ["home", "general", "shortcuts", "apps", "model", "privacy"];

pub(crate) fn sample_overview() -> Overview {
    Overview {
        style: Vec::new(),
        status: "Ready".into(),
        stats: StatsView {
            today: Counts {
                shown: 48,
                accepted: 17,
                accepted_words: 9,
                used: 26,
                words: 112,
                chars: 640,
            },
            total: Counts {
                shown: 12_480,
                accepted: 3_912,
                accepted_words: 2_207,
                used: 5_604,
                words: 41_530,
                chars: 236_400,
            },
            days: [0, 0, 40, 220, 310, 0, 90, 180, 260, 140, 75, 330, 205, 112],
        },
        pause: None,
        app: Some(ForegroundApp {
            name: "Slack".into(),
            access: Access::On,
            switch: Some(Setting::Block("Slack".into())),
        }),
        start_with_windows: true,
    }
}

pub(crate) fn home(config: &Config, overview: &Overview, rival: Option<Rival>) -> SettingsWindow {
    let window = SettingsWindow::open(config, ModelPreset::Standard, overview).unwrap();
    window.set_rival(rival);
    window
}

fn page_shot(name: &str, window: &SettingsWindow, page: Page, (dpi, fallback): (u32, [u8; 3])) {
    window.show_page(page);
    park(window.hwnd(), dpi);
    resize(window.hwnd(), dpi, FULL_PAGE);
    capture(window.hwnd(), name, fallback);
}

pub(crate) fn pages(tag: &str, dpi: u32, fallback: [u8; 3]) {
    let config = Config {
        blocked_apps: vec!["Slack".into(), "Figma".into(), "Notion".into()],
        allowed_apps: vec!["Code".into()],
        ghost_opacity: 70,
        max_words: 10,
        ..Config::default()
    };
    let window = home(&config, &sample_overview(), None);
    for (page, label) in Page::ALL.into_iter().zip(PAGE_NAMES) {
        let name = format!("{label}-{tag}");
        page_shot(&name, &window, page, (dpi, fallback));
    }
}

pub(crate) fn fits_small_laptop(theme: &str, fallback: [u8; 3]) {
    let window = home(&Config::default(), &sample_overview(), None);
    window.show_page(Page::General);
    park(window.hwnd(), 144);
    resize(window.hwnd(), 144, SMALL_LAPTOP_CLIENT);
    capture(
        window.hwnd(),
        &format!("laptop-1366x768-150-{theme}"),
        fallback,
    );
}

pub(crate) fn variants(theme: &str, fallback: [u8; 3]) {
    let quiet = Overview {
        status: "Ready".into(),
        ..Overview::default()
    };
    let paused = Overview {
        pause: Some(Pause::Until("14:35".into())),
        status: "Paused until 14:35".into(),
        ..sample_overview()
    };
    let remote = Config {
        remote_url: Some("https://api.example.com/v1".into()),
        ..Config::default()
    };
    let cases = [
        ("home-new", Page::Home, Config::default(), quiet, None),
        ("home-paused", Page::Home, Config::default(), paused, None),
        (
            "home-rival",
            Page::Home,
            Config::default(),
            sample_overview(),
            Some(Rival::TextSuggestions),
        ),
        ("model-remote", Page::Model, remote, sample_overview(), None),
    ];
    for (name, page, config, overview, rival) in cases {
        let window = home(&config, &overview, rival);
        page_shot(&format!("{name}-{theme}-96"), &window, page, (96, fallback));
    }
}
