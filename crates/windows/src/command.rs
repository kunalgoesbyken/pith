use pith_core::{Access, PauseSpan, Setting};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Pause(PauseSpan),
    Resume,
    ChangeApp,
    Update,
    Open,
    Quit,
    ResumeApp(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForegroundApp {
    pub name: String,
    pub access: Access,
    pub switch: Option<Setting>,
}
