use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use pith_core::{Spelling, log};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Globalization::{
    CORRECTIVE_ACTION_GET_SUGGESTIONS, CORRECTIVE_ACTION_REPLACE, ISpellChecker,
    ISpellCheckerFactory, SpellCheckerFactory,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::core::{PCWSTR, PWSTR};

use crate::util::wide;

struct Check {
    generation: u64,
    language: String,
    word: String,
}

pub struct Speller {
    requests: Sender<Check>,
}

impl Speller {
    pub fn spawn(on_result: impl Fn(u64, Spelling) + Send + 'static) -> Self {
        let (requests, queue) = channel();
        std::thread::Builder::new()
            .name("pith-spell".into())
            .spawn(move || serve(queue, on_result))
            .expect("spawn spell thread");
        Self { requests }
    }

    pub fn check(&self, generation: u64, language: String, word: String) {
        let _ = self.requests.send(Check {
            generation,
            language,
            word,
        });
    }
}

fn factory() -> Option<ISpellCheckerFactory> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()
    }
}

fn serve(queue: Receiver<Check>, on_result: impl Fn(u64, Spelling)) {
    let Some(factory) = factory() else {
        return log("spell checking unavailable");
    };
    let mut checkers: HashMap<String, Option<ISpellChecker>> = HashMap::new();
    while let Ok(request) = queue.recv() {
        let checker = checkers
            .entry(request.language.clone())
            .or_insert_with(|| checker_for(&factory, &request.language));
        let spelling = checker
            .as_ref()
            .map_or(Spelling::Correct, |checker| verdict(checker, &request.word));
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

fn verdict(checker: &ISpellChecker, word: &str) -> Spelling {
    let text = wide(word);
    let text = PCWSTR(text.as_ptr());
    let Ok(errors) = (unsafe { checker.Check(text) }) else {
        return Spelling::Correct;
    };
    let (mut first, mut second) = (None, None);
    let found = unsafe { errors.Next(&mut first) } == S_OK;
    let extra = unsafe { errors.Next(&mut second) } == S_OK;
    let (true, false, Some(error)) = (found, extra, first) else {
        return Spelling::Correct;
    };
    let whole = word.encode_utf16().count();
    let span = unsafe { (error.StartIndex().ok(), error.Length().ok()) };
    let spans_word = span == (Some(0), Some(whole as u32));
    if !spans_word {
        return Spelling::Correct;
    }
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
