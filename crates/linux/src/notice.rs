use notify_rust::Notification;
use pith_core::log;

const APP: &str = "Pith";

pub fn notify(body: String) {
    std::thread::spawn(move || {
        let shown = Notification::new()
            .appname(APP)
            .summary(APP)
            .body(&body)
            .show();
        if let Err(error) = shown {
            log(&format!("notice not shown: {error}"));
        }
    });
}
