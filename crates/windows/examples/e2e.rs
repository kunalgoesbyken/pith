use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use pith_core::{Action, Config};
use pith_infer::{Backend, Worker};
use pith_windows::{
    ACCEPT_ALL, ACCEPT_WORD, DISMISS, Inbound, InsertMode, Options, Source, Waker, run,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::{EM_REPLACESEL, EM_SETSEL};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

unsafe extern "system" fn host_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn spawn_test_window() -> (isize, isize) {
    let (tx, rx) = channel();
    std::thread::spawn(move || unsafe {
        let _ = LoadLibraryW(w!("Msftedit.dll"));
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PithE2EHost");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let host = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class,
            w!("Pith e2e target"),
            WS_OVERLAPPEDWINDOW,
            60,
            60,
            900,
            220,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0004),
            10,
            10,
            860,
            150,
            Some(host),
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let _ = ShowWindow(host, SW_SHOWNOACTIVATE);
        tx.send((host.0 as isize, edit.0 as isize)).unwrap();
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    rx.recv().unwrap()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn send_text(edit: isize, text: &str) {
    let text = wide(text);
    unsafe {
        SendMessageW(
            HWND(edit as _),
            EM_REPLACESEL,
            Some(WPARAM(1)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
    }
}

fn set_text(edit: isize, text: &str) {
    let text = wide(text);
    unsafe {
        SendMessageW(
            HWND(edit as _),
            WM_SETTEXT,
            Some(WPARAM(0)),
            Some(LPARAM(text.as_ptr() as isize)),
        );
        SendMessageW(
            HWND(edit as _),
            EM_SETSEL,
            Some(WPARAM(usize::MAX)),
            Some(LPARAM(-1)),
        );
    }
}

fn select(edit: isize, at: usize) {
    unsafe {
        SendMessageW(
            HWND(edit as _),
            EM_SETSEL,
            Some(WPARAM(at)),
            Some(LPARAM(at as isize)),
        );
    }
}

fn get_text(edit: isize) -> String {
    let mut buffer = vec![0u16; 4096];
    let length = unsafe {
        SendMessageW(
            HWND(edit as _),
            WM_GETTEXT,
            Some(WPARAM(buffer.len())),
            Some(LPARAM(buffer.as_mut_ptr() as isize)),
        )
    };
    String::from_utf16_lossy(&buffer[..length.0 as usize])
}

fn type_slowly(edit: isize, text: &str, gap_ms: u64) {
    for character in text.chars() {
        send_text(edit, &character.to_string());
        std::thread::sleep(Duration::from_millis(gap_ms));
    }
}

fn wait_for(
    actions: &Receiver<Action>,
    timeout: Duration,
    predicate: impl Fn(&Action) -> bool,
) -> Option<(Action, Duration)> {
    let started = Instant::now();
    while let Some(remaining) = timeout.checked_sub(started.elapsed()) {
        match actions.recv_timeout(remaining) {
            Ok(action) if predicate(&action) => return Some((action, started.elapsed())),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

fn drain(actions: &Receiver<Action>) {
    while actions.try_recv().is_ok() {}
}

fn settle(actions: &Receiver<Action>, quiet: Duration) -> Vec<Action> {
    let mut seen = Vec::new();
    while let Ok(action) = actions.recv_timeout(quiet) {
        seen.push(action);
    }
    seen
}

fn last_show(seen: &[Action]) -> Option<String> {
    seen.iter().rev().find(|a| is_show(a)).map(shown_text)
}

fn screenshot(host: isize, path: &PathBuf) {
    unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = GetWindowRect(HWND(host as _), &mut rect);
        let (x, y, width, height) = (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let _ = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(memory, previous);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        let mut file = Vec::with_capacity(54 + pixels.len());
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&54u32.to_le_bytes());
        file.extend_from_slice(&40u32.to_le_bytes());
        file.extend_from_slice(&width.to_le_bytes());
        file.extend_from_slice(&height.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&[0; 24]);
        file.extend_from_slice(&pixels);
        let _ = std::fs::write(path, file);
    }
}

struct Report {
    passed: usize,
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: String) {
        if ok {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        println!("[{}] {name}: {detail}", if ok { "PASS" } else { "FAIL" });
    }
}

fn shown_text(action: &Action) -> String {
    match action {
        Action::Show { text, .. } => text.clone(),
        _ => String::new(),
    }
}

fn is_show(action: &Action) -> bool {
    matches!(action, Action::Show { .. })
}

fn drive(
    edit: isize,
    host: isize,
    actions: Receiver<Action>,
    inbound: Sender<Inbound>,
    waker: Waker,
    ready: Receiver<()>,
    out_dir: PathBuf,
) {
    let key = |id: i32| {
        let _ = inbound.send(Inbound::Key(id));
        waker.wake();
    };
    let mut report = Report {
        passed: 0,
        failed: 0,
    };
    let _ = ready.recv_timeout(Duration::from_secs(60));
    let wait = Duration::from_secs(4);

    let scenarios = [
        "Hi Sarah, thanks for sending the revised contract. I reviewed the payment terms and they look",
        "Quick update on the landing page: the new hero section is live on staging and I would love your",
        "Please let me know if you have any",
    ];
    let mut latencies = Vec::new();
    for (index, sentence) in scenarios.iter().enumerate() {
        set_text(edit, "");
        std::thread::sleep(Duration::from_millis(300));
        drain(&actions);
        type_slowly(edit, sentence, 25);
        let shown = wait_for(&actions, wait, is_show);
        let suggestion = shown
            .as_ref()
            .map(|(a, _)| shown_text(a))
            .unwrap_or_default();
        if let Some((_, latency)) = &shown {
            latencies.push(latency.as_millis());
        }
        report.check(
            &format!("scenario {index} shows a suggestion"),
            shown.is_some(),
            format!("{suggestion:?} after {:?}", shown.as_ref().map(|s| s.1)),
        );
        if index == 0 && shown.is_some() {
            std::thread::sleep(Duration::from_millis(150));
            screenshot(host, &out_dir.join("e2e-ghost.bmp"));
        }
    }

    set_text(edit, "");
    drain(&actions);
    type_slowly(edit, scenarios[1], 10);
    let seen = settle(&actions, Duration::from_millis(900));
    if let Some(suggestion) = last_show(&seen) {
        key(ACCEPT_WORD);
        let seen = settle(&actions, Duration::from_millis(400));
        let text = get_text(edit);
        let word = seen
            .iter()
            .find_map(|a| {
                if let Action::Insert(w) = a {
                    Some(w.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        report.check(
            "accept word inserts the next word",
            !word.is_empty() && text.ends_with(&word) && suggestion.starts_with(&word),
            format!(
                "suggestion {suggestion:?}, word {word:?}, text tail {:?}",
                tail(&text)
            ),
        );
        let rest = suggestion[word.len().min(suggestion.len())..].to_string();
        if rest.trim().is_empty() {
            report.check(
                "single-word suggestion is fully consumed by accept word",
                seen.iter().any(|a| matches!(a, Action::Hide)),
                format!("{seen:?}"),
            );
        } else {
            key(ACCEPT_ALL);
            let seen = settle(&actions, Duration::from_millis(400));
            let text = get_text(edit);
            let ok = seen
                .iter()
                .any(|a| matches!(a, Action::Insert(t) if *t == rest))
                && text.ends_with(rest.as_str());
            report.check(
                "accept all inserts the rest",
                ok,
                format!("rest {rest:?}, text tail {:?}", tail(&text)),
            );
        }
        let next = wait_for(&actions, wait, is_show);
        println!(
            "[INFO] follow-up after accepting: {:?}",
            next.map(|(a, d)| (shown_text(&a), d))
        );
    } else {
        report.check("accept flow", false, "no suggestion".into());
    }

    set_text(edit, "");
    drain(&actions);
    type_slowly(edit, scenarios[2], 10);
    let seen = settle(&actions, Duration::from_millis(900));
    if let Some(suggestion) = last_show(&seen).filter(|s| s.chars().count() > 3) {
        let typed: String = suggestion.chars().take(2).collect();
        type_slowly(edit, &typed, 30);
        let seen = settle(&actions, Duration::from_millis(500));
        let requested = seen
            .iter()
            .any(|a| matches!(a, Action::StartCompletion { .. }));
        let expected = suggestion.strip_prefix(typed.as_str()).map(str::to_string);
        let ok = !requested && last_show(&seen) == expected;
        report.check("typing through shrinks the suggestion without a new request", ok, format!("suggestion {suggestion:?}, typed {typed:?}, last shown {:?}, new request {requested}", last_show(&seen)));
        key(DISMISS);
        let seen = settle(&actions, Duration::from_millis(900));
        let hidden = seen.iter().any(|a| matches!(a, Action::Hide));
        let reshown = seen.iter().any(is_show);
        report.check(
            "escape dismisses and stays dismissed",
            hidden && !reshown,
            format!("hidden {hidden}, reshown {reshown}"),
        );
    } else {
        report.check(
            "typing-through flow",
            false,
            "no multi-char suggestion".into(),
        );
    }

    set_text(edit, "We should schedule the review for next week");
    std::thread::sleep(Duration::from_millis(200));
    drain(&actions);
    select(edit, 10);
    let shown = wait_for(&actions, Duration::from_millis(1500), is_show);
    report.check(
        "no suggestion when text follows the caret",
        shown.is_none(),
        format!("{:?}", shown.map(|(a, _)| shown_text(&a))),
    );

    report_memory();
    if std::env::var("PITH_IDLE_UNLOAD").is_ok() {
        std::thread::sleep(Duration::from_secs(4));
        print!("[INFO] after idle unload: ");
        report_memory();
        set_text(edit, "");
        drain(&actions);
        type_slowly(edit, scenarios[2], 10);
        let shown = wait_for(&actions, Duration::from_secs(8), is_show);
        report.check(
            "suggestion returns after the model was unloaded",
            shown.is_some(),
            format!("{:?}", shown.map(|(a, d)| (shown_text(&a), d))),
        );
    }
    latencies.sort_unstable();
    println!("\nlast keystroke -> ghost text latency (ms): {latencies:?}");
    println!("RESULT: {} passed, {} failed", report.passed, report.failed);
    let _ = inbound.send(Inbound::Quit);
    waker.wake();
}

fn report_memory() {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
    }
    println!(
        "[INFO] working set {} MB, peak {} MB, private {} MB",
        counters.WorkingSetSize >> 20,
        counters.PeakWorkingSetSize >> 20,
        counters.PagefileUsage >> 20
    );
}

fn tail(text: &str) -> String {
    text.chars()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(args.next().expect("model path"));
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let (host, edit) = spawn_test_window();
    let config = Config::default();
    let (sender, receiver) = channel();
    let waker = Waker::default();
    let (ready_tx, ready_rx) = channel();
    let (done_sender, done_waker) = (sender.clone(), waker.clone());
    let threads = config.resolved_threads(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    );
    let context_tokens = config.context_tokens;
    let idle_unload_secs = std::env::var("PITH_IDLE_UNLOAD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let worker = Worker::spawn(
        Box::new(move || {
            Ok(Backend::Local {
                model_path: model.clone(),
                threads,
                context_tokens,
                idle_unload_secs,
            })
        }),
        Box::new(move |status| {
            println!("worker ready: {status:?}");
            let _ = ready_tx.send(());
        }),
        Box::new(move |generation, result| {
            let _ = done_sender.send(Inbound::Done(generation, result));
            done_waker.wake();
        }),
    );
    let (observer, actions) = channel();
    let driver_inbound = sender.clone();
    let driver_waker = waker.clone();
    std::thread::spawn(move || {
        drive(
            edit,
            host,
            actions,
            driver_inbound,
            driver_waker,
            ready_rx,
            out_dir,
        )
    });
    let options = Options {
        source: Source::Window(edit),
        insert: InsertMode::Window(edit),
        tray: false,
        poll_ms: 50,
        config_path: None,
        observer: Some(observer),
        global_keys: false,
    };
    run(config, worker, (sender, receiver), waker, options).expect("run");
    let _ = host;
}
