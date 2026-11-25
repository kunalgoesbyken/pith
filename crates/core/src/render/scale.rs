pub const BASE_DPI: u32 = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale(pub u32);

impl Scale {
    pub fn px(self, logical: i32) -> i32 {
        (logical * self.0 as i32 + BASE_DPI as i32 / 2) / BASE_DPI as i32
    }

    pub fn size(self, logical: f32) -> f32 {
        logical * self.0 as f32 / BASE_DPI as f32
    }
}
