#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

impl Block {
    pub const fn capsule(x: f32, y: f32, width: f32, height: f32) -> Self {
        let radius = if width < height { width } else { height } / 2.0;
        Self {
            x,
            y,
            width,
            height,
            radius,
        }
    }

    pub const fn square(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
            radius: 0.0,
        }
    }

    pub fn scaled(self, scale: f32, origin: (f32, f32)) -> Self {
        Self {
            x: origin.0 + self.x * scale,
            y: origin.1 + self.y * scale,
            width: self.width * scale,
            height: self.height * scale,
            radius: self.radius * scale,
        }
    }

    pub fn contains(&self, (px, py): (f32, f32)) -> bool {
        let half = (self.width / 2.0, self.height / 2.0);
        let dx = (px - self.x - half.0).abs() - (half.0 - self.radius);
        let dy = (py - self.y - half.1).abs() - (half.1 - self.radius);
        dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) <= self.radius
    }
}

pub const MARK_WIDTH: f32 = 174.0;
pub const MARK_HEIGHT: f32 = 164.0;

pub const TRAIL: [Block; 3] = [
    Block::capsule(0.0, 0.0, 28.0, 164.0),
    Block::capsule(50.0, 68.0, 80.0, 28.0),
    Block::capsule(146.0, 68.0, 28.0, 28.0),
];
