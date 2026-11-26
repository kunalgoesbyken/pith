use windows::Win32::Graphics::Gdi::{CLEARTYPE_QUALITY, CreateFontIndirectW, HFONT, LOGFONTW};

use super::canvas::family_name;
use super::metrics::{Role, Scale};

pub fn gdi_font(role: Role, scale: Scale) -> Option<HFONT> {
    let mut face = [0u16; 32];
    let family = family_name(role.typeface());
    let length = family.len().min(face.len() - 1);
    face[..length].copy_from_slice(&family[..length]);
    let font = LOGFONTW {
        lfHeight: -scale.px(role.size() as i32),
        lfWeight: role.weight(),
        lfQuality: CLEARTYPE_QUALITY,
        lfFaceName: face,
        ..Default::default()
    };
    let created = unsafe { CreateFontIndirectW(&font) };
    (!created.is_invalid()).then_some(created)
}
