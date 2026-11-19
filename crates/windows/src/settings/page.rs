#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Home,
    General,
    Shortcuts,
    Apps,
    Model,
    Privacy,
}

impl Page {
    pub const ALL: [Self; 6] = [
        Self::Home,
        Self::General,
        Self::Shortcuts,
        Self::Apps,
        Self::Model,
        Self::Privacy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::General => "General",
            Self::Shortcuts => "Shortcuts",
            Self::Apps => "Apps",
            Self::Model => "Model",
            Self::Privacy => "Privacy & data",
        }
    }
}
