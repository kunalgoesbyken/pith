use serde::Serialize;

const CHANNEL_SHIFTS: [u32; 3] = [16, 8, 0];
const BGR_SHIFTS: [u32; 3] = [0, 8, 16];
const FULL: u32 = 255;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rgb(pub u32);

impl Rgb {
    pub const WHITE: Rgb = Rgb(0xFF_FFFF);
    pub const BLACK: Rgb = Rgb(0);

    pub fn channels(self) -> [u32; 3] {
        CHANNEL_SHIFTS.map(|shift| (self.0 >> shift) & FULL)
    }

    pub fn from_channels(channels: [u32; 3]) -> Self {
        Self(pack(CHANNEL_SHIFTS, channels))
    }

    pub fn unit_channels(self) -> [f32; 3] {
        self.channels().map(|channel| channel as f32 / FULL as f32)
    }

    pub fn from_bgr(color: u32) -> Self {
        Self::from_channels(BGR_SHIFTS.map(|shift| (color >> shift) & FULL))
    }

    pub fn bgr(self) -> u32 {
        pack(BGR_SHIFTS, self.channels())
    }

    pub fn premultiplied(self, alpha: u32) -> u32 {
        argb(alpha, self.channels().map(|channel| channel * alpha / FULL))
    }
}

fn pack(shifts: [u32; 3], channels: [u32; 3]) -> u32 {
    shifts
        .into_iter()
        .zip(channels)
        .map(|(shift, channel)| (channel & FULL) << shift)
        .sum()
}

pub fn argb(alpha: u32, channels: [u32; 3]) -> u32 {
    (alpha.min(FULL) << 24) | pack(CHANNEL_SHIFTS, channels)
}
