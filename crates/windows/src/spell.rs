use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use pith_core::{Spelling, log};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Globalization::{
    CORRECTIVE_ACTION_GET_SUGGESTIONS, CORRECTIVE_ACTION_REPLACE, ISpellChecker,
    ISpellCheckerFactory, ISpellingError, SpellCheckerFactory,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{PCWSTR, PWSTR};

use crate::language::input_language;
use crate::util::wide;

const WARM_UP_WORD: &str = "the";
const FACTORY_RETRY_AFTER: Duration = Duration::from_secs(30);

struct Check {
    generation: u64,
    language: String,
    word: String,
}

pub struct Speller {
    requests: Sender<Check>,
}

impl Speller {
    pub fn spawn(on_result: impl Fn(u64, Spelling) + Send + 'static) -> Option<Self> {
        let (requests, queue) = channel();
        std::thread::Builder::new()
            .name("pith-spell".into())
            .spawn(move || serve(queue, on_result))
            .ok()?;
        Some(Self { requests })
    }

    pub fn check(&self, generation: u64, language: String, word: String) {
        let _ = self.requests.send(Check {
            generation,
            language,
            word,
        });
    }
}

struct Apartment;

impl Apartment {
    fn enter() -> Option<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .is_ok()
            .then_some(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn create_factory() -> windows::core::Result<ISpellCheckerFactory> {
    unsafe { CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER) }
}

#[derive(Default)]
struct Checkers {
    factory: Option<ISpellCheckerFactory>,
    retry_at: Option<Instant>,
    by_language: HashMap<String, ISpellChecker>,
    unsupported: HashMap<String, Instant>,
}

impl Checkers {
    fn connect(&mut self) {
        let due = self.retry_at.is_none_or(|at| Instant::now() >= at);
        if self.factory.is_some() || !due {
            return;
        }
        match create_factory() {
            Ok(factory) => {
                self.factory = Some(factory);
                self.retry_at = None;
            }
            Err(error) => {
                log(&format!("spell checking unavailable: {error}"));
                self.retry_at = Some(Instant::now() + FACTORY_RETRY_AFTER);
            }
        }
    }

    fn throttled(&self, language: &str) -> bool {
        self.unsupported
            .get(language)
            .is_some_and(|retry_at| Instant::now() < *retry_at)
    }

    fn postpone(&mut self, language: &str) {
        let retry_at = Instant::now() + FACTORY_RETRY_AFTER;
        if self
            .unsupported
            .insert(language.to_string(), retry_at)
            .is_none()
        {
            log(&format!("no spell checker for {language}"));
        }
    }

    fn checker(&mut self, language: &str) -> Option<&ISpellChecker> {
        let factory = self.factory.as_ref()?;
        if !self.by_language.contains_key(language) && !self.throttled(language) {
            match checker_for(factory, language) {
                Some(checker) => {
                    self.unsupported.remove(language);
                    self.by_language.insert(language.to_string(), checker);
                }
                None => self.postpone(language),
            }
        }
        self.by_language.get(language)
    }

    fn spelling(&mut self, language: &str, word: &str) -> Spelling {
        self.connect();
        self.checker(language)
            .map_or(Spelling::Correct, |checker| verdict(checker, word))
    }
}

fn serve(queue: Receiver<Check>, on_result: impl Fn(u64, Spelling)) {
    let apartment = Apartment::enter();
    if apartment.is_none() {
        log("spell checking unavailable: COM did not start");
    }
    let mut checkers = Checkers::default();
    if apartment.is_some()
        && let Some(language) = input_language(unsafe { GetForegroundWindow() })
    {
        checkers.spelling(&language, WARM_UP_WORD);
    }
    while let Ok(request) = queue.recv() {
        let spelling = match apartment {
            Some(_) => checkers.spelling(&request.language, &request.word),
            None => Spelling::Correct,
        };
        on_result(request.generation, spelling);
    }
}

fn checker_for(factory: &ISpellCheckerFactory, language: &str) -> Option<ISpellChecker> {
    let primary = language.split('-').next().unwrap_or(language);
    [language, primary].into_iter().find_map(|tag| {
        let tag = wide(tag);
        let tag = PCWSTR(tag.as_ptr());
        let supported = unsafe { factory.IsSupported(tag) }.ok()?.as_bool();
        supported
            .then(|| unsafe { factory.CreateSpellChecker(tag) }.ok())
            .flatten()
    })
}

fn take_string(text: PWSTR) -> Option<String> {
    let owned = unsafe { text.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
    owned
}

fn top_suggestion(checker: &ISpellChecker, word: PCWSTR) -> Option<String> {
    let suggestions = unsafe { checker.Suggest(word) }.ok()?;
    let mut first = [PWSTR::null()];
    let mut fetched = 0;
    let status = unsafe { suggestions.Next(&mut first, Some(&mut fetched)) };
    (status == S_OK && fetched == 1)
        .then(|| take_string(first[0]))
        .flatten()
}

fn only_error(checker: &ISpellChecker, text: PCWSTR) -> Option<ISpellingError> {
    let errors = unsafe { checker.Check(text) }.ok()?;
    let (mut first, mut second) = (None, None);
    let found = unsafe { errors.Next(&mut first) } == S_OK;
    let extra = unsafe { errors.Next(&mut second) } == S_OK;
    first.filter(|_| found && !extra)
}

fn spans_word(error: &ISpellingError, word: &str) -> bool {
    let whole = u32::try_from(word.encode_utf16().count()).ok();
    let span = unsafe { (error.StartIndex().ok(), error.Length().ok()) };
    span == (Some(0), whole)
}

fn verdict(checker: &ISpellChecker, word: &str) -> Spelling {
    let text = wide(word);
    let text = PCWSTR(text.as_ptr());
    let Some(error) = only_error(checker, text).filter(|error| spans_word(error, word)) else {
        return Spelling::Correct;
    };
    match unsafe { error.CorrectiveAction() } {
        Ok(CORRECTIVE_ACTION_REPLACE) => unsafe { error.Replacement() }
            .ok()
            .and_then(take_string)
            .map_or(Spelling::Correct, Spelling::Autocorrect),
        Ok(CORRECTIVE_ACTION_GET_SUGGESTIONS) => {
            top_suggestion(checker, text).map_or(Spelling::Correct, Spelling::Suggested)
        }
        _ => Spelling::Correct,
    }
}
