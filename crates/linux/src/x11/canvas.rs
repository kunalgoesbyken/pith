use std::sync::Arc;

use pith_core::{Point, Size};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt, CreateGCAux,
    CreateWindowAux, Gcontext, ImageFormat, PropMode, Screen, StackMode, VisualClass, Visualid,
    Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

use super::display::{Display, DisplayError};

pub const OVERLAY_NAME: &str = "PithGhostText";
const ALPHA_DEPTH: u8 = 32;
const ALPHA_RED_MASK: u32 = 0x00FF_0000;
const REQUEST_HEADER_BYTES: usize = 32;
const BYTES_PER_PIXEL: usize = 4;

pub struct Canvas {
    display: Arc<Display>,
    window: Window,
    gc: Gcontext,
    depth: u8,
    has_alpha: bool,
    visible: bool,
}

fn alpha_visual(screen: &Screen) -> Option<Visualid> {
    screen
        .allowed_depths
        .iter()
        .filter(|depth| depth.depth == ALPHA_DEPTH)
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.class == VisualClass::TRUE_COLOR && visual.red_mask == ALPHA_RED_MASK)
        .map(|visual| visual.visual_id)
}

impl Canvas {
    pub fn create(display: Arc<Display>) -> Result<Self, DisplayError> {
        let connection = &display.connection;
        let screen = &connection.setup().roots[display.screen];
        let alpha = alpha_visual(screen);
        let (depth, visual) = alpha.map_or((screen.root_depth, screen.root_visual), |visual| {
            (ALPHA_DEPTH, visual)
        });
        let colormap = connection.generate_id()?;
        connection.create_colormap(ColormapAlloc::NONE, colormap, display.root, visual)?;
        let window = connection.generate_id()?;
        connection.create_window(
            depth,
            window,
            display.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap),
        )?;
        connection.shape_rectangles(
            SO::SET,
            SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?;
        let class = [OVERLAY_NAME, OVERLAY_NAME].join("\0") + "\0";
        connection.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            OVERLAY_NAME.as_bytes(),
        )?;
        connection.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            class.as_bytes(),
        )?;
        let gc = connection.generate_id()?;
        connection.create_gc(gc, window, &CreateGCAux::new())?;
        connection.flush()?;
        Ok(Self {
            display,
            window,
            gc,
            depth,
            has_alpha: alpha.is_some(),
            visible: false,
        })
    }

    pub fn has_alpha(&self) -> bool {
        self.has_alpha
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn present(&mut self, at: Point, size: Size, pixels: &[u32]) -> Result<(), DisplayError> {
        let connection = &self.display.connection;
        let width = size.width as usize;
        connection.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(at.x)
                .y(at.y)
                .width(size.width as u32)
                .height(size.height as u32)
                .stack_mode(StackMode::ABOVE),
        )?;
        if !self.visible {
            connection.map_window(self.window)?;
            self.visible = true;
        }
        let capacity = connection
            .maximum_request_bytes()
            .saturating_sub(REQUEST_HEADER_BYTES);
        let rows_per_request = (capacity / (width * BYTES_PER_PIXEL)).max(1);
        for (band, rows) in pixels.chunks(width * rows_per_request).enumerate() {
            let data: Vec<u8> = rows.iter().flat_map(|pixel| pixel.to_ne_bytes()).collect();
            connection.put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                width as u16,
                (rows.len() / width) as u16,
                0,
                (band * rows_per_request) as i16,
                0,
                self.depth,
                &data,
            )?;
        }
        connection.flush()?;
        Ok(())
    }

    pub fn hide(&mut self) -> Result<(), DisplayError> {
        if !self.visible {
            return Ok(());
        }
        self.visible = false;
        self.display.connection.unmap_window(self.window)?;
        self.display.connection.flush()?;
        Ok(())
    }
}
