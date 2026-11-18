use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelPreset {
    Small,
    Standard,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hardware {
    pub ram_mib: u64,
    pub cores: usize,
}

struct Profile {
    label: &'static str,
    loaded_mib: u64,
    other_apps_mib: u64,
    min_cores: usize,
}

const SMALL: Profile = Profile {
    label: "Small",
    loaded_mib: 370,
    other_apps_mib: 0,
    min_cores: 0,
};

const STANDARD: Profile = Profile {
    label: "Standard",
    loaded_mib: 590,
    other_apps_mib: 5 * 1024,
    min_cores: 4,
};

const LARGE: Profile = Profile {
    label: "Large",
    loaded_mib: 1270,
    other_apps_mib: 12 * 1024,
    min_cores: 12,
};

impl ModelPreset {
    pub const ALL: [Self; 3] = [Self::Small, Self::Standard, Self::Large];

    fn profile(self) -> &'static Profile {
        match self {
            Self::Small => &SMALL,
            Self::Standard => &STANDARD,
            Self::Large => &LARGE,
        }
    }

    pub fn label(self) -> &'static str {
        self.profile().label
    }

    pub fn loaded_mib(self) -> u64 {
        self.profile().loaded_mib
    }

    fn fits(self, hardware: Hardware) -> bool {
        let profile = self.profile();
        hardware.ram_mib >= profile.loaded_mib + profile.other_apps_mib
            && hardware.cores >= profile.min_cores
    }
}

impl Hardware {
    pub fn recommended_model(self) -> ModelPreset {
        ModelPreset::ALL
            .into_iter()
            .rev()
            .find(|preset| preset.fits(self))
            .unwrap_or(ModelPreset::Small)
    }
}
