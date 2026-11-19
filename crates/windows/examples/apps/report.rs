use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

const RESULTS_FILE: &str = "results.md";
const RESULTS_HEADER: &str = "| App | Field | Variant | Reads text before caret | Caret | Ghost suggestion | Accept inserts | Result |\n|---|---|---|---|---|---|---|---|\n";
const NONE: &str = "-";

#[derive(Clone, Copy)]
pub enum App {
    Chrome,
    Edge,
    Electron,
    Notepad,
}

impl App {
    pub fn from_arg(name: &str) -> Option<Self> {
        match name {
            "chrome" => Some(App::Chrome),
            "edge" => Some(App::Edge),
            "electron" => Some(App::Electron),
            "notepad" => Some(App::Notepad),
            _ => None,
        }
    }

    pub fn optional(self) -> bool {
        matches!(self, App::Electron | App::Notepad)
    }

    pub fn cases(self) -> Vec<Case> {
        match self {
            App::Notepad => vec![Case {
                field: Field::Native,
                variant: Variant::Default,
            }],
            App::Chrome | App::Edge | App::Electron => [Field::Textarea, Field::Editable]
                .into_iter()
                .flat_map(|field| {
                    [Variant::Default, Variant::AccessibilityFlag]
                        .into_iter()
                        .map(move |variant| Case { field, variant })
                })
                .collect(),
        }
    }
}

impl fmt::Display for App {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            App::Chrome => "chrome",
            App::Edge => "edge",
            App::Electron => "electron",
            App::Notepad => "notepad",
        })
    }
}

#[derive(Clone, Copy)]
pub enum Field {
    Textarea,
    Editable,
    Native,
}

impl fmt::Display for Field {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Field::Textarea => "textarea",
            Field::Editable => "editable",
            Field::Native => "native",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Default,
    AccessibilityFlag,
}

impl fmt::Display for Variant {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Variant::Default => "default",
            Variant::AccessibilityFlag => "accessibility-flag",
        })
    }
}

#[derive(Clone, Copy)]
pub struct Case {
    pub field: Field,
    pub variant: Variant,
}

pub enum Outcome {
    Pass,
    Fail(String),
    Skipped(&'static str),
}

impl fmt::Display for Outcome {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Pass => out.write_str("pass"),
            Outcome::Fail(reason) => write!(out, "FAIL: {reason}"),
            Outcome::Skipped(reason) => write!(out, "skipped: {reason}"),
        }
    }
}

pub struct Row {
    app: App,
    case: Case,
    pub reads: String,
    pub caret: String,
    pub ghost: String,
    pub accept: String,
    outcome: Outcome,
}

impl Row {
    pub fn new(app: App, case: Case) -> Self {
        Self {
            app,
            case,
            reads: NONE.to_string(),
            caret: NONE.to_string(),
            ghost: NONE.to_string(),
            accept: NONE.to_string(),
            outcome: Outcome::Pass,
        }
    }

    pub fn skipped(app: App, case: Case, reason: &'static str) -> Self {
        Self {
            outcome: Outcome::Skipped(reason),
            ..Self::new(app, case)
        }
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        match &mut self.outcome {
            Outcome::Pass => self.outcome = Outcome::Fail(reason),
            Outcome::Fail(existing) => {
                existing.push_str("; ");
                existing.push_str(&reason);
            }
            Outcome::Skipped(_) => {}
        }
    }

    pub fn failed(&self) -> bool {
        matches!(self.outcome, Outcome::Fail(_))
    }

    fn markdown(&self) -> String {
        let cells = [
            self.app.to_string(),
            self.case.field.to_string(),
            self.case.variant.to_string(),
            self.reads.clone(),
            self.caret.clone(),
            self.ghost.clone(),
            self.accept.clone(),
            self.outcome.to_string(),
        ];
        let cells: Vec<String> = cells
            .iter()
            .map(|cell| cell.replace(['|', '\n'], " "))
            .collect();
        format!("| {} |\n", cells.join(" | "))
    }
}

pub fn record(out: &Path, row: &Row) {
    let line = row.markdown();
    print!("{line}");
    let path = out.join(RESULTS_FILE);
    let fresh = !path.exists();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        if fresh {
            let _ = file.write_all(RESULTS_HEADER.as_bytes());
        }
        let _ = file.write_all(line.as_bytes());
    }
}
