use std::sync::{Arc, PoisonError, RwLock};

use pith_core::{BASE_DPI, Rect, Scale};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{ConnectionExt as _, NotifyMask};
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask,
};
use x11rb::resource_manager::new_from_resource_manager;

use super::display::{Display, DisplayError};

const DPI_RESOURCE: &str = "Xft.dpi";
const DPI_CLASS: &str = "Xft.Dpi";
const MIN_DPI: f64 = 48.0;
const MAX_DPI: f64 = 480.0;
const RANDR_VERSION: (u32, u32) = (1, 5);
const XFIXES_VERSION: (u32, u32) = (5, 0);

#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub work: Rect,
    pub scale: Scale,
}

struct View {
    areas: Vec<Rect>,
    scale: Scale,
    compositing: bool,
}

#[derive(Clone)]
pub struct Desktop {
    display: Arc<Display>,
    compositor: Atom,
    view: Arc<RwLock<View>>,
}

fn clamp_dpi(dpi: f64) -> u32 {
    if dpi.is_finite() && dpi > 0.0 {
        dpi.clamp(MIN_DPI, MAX_DPI).round() as u32
    } else {
        BASE_DPI
    }
}

fn distance_to(area: Rect, x: i32, y: i32) -> i64 {
    let dx = i64::from((area.x - x).max(0).max(x - (area.x + area.width)));
    let dy = i64::from((area.y - y).max(0).max(y - (area.y + area.height)));
    dx * dx + dy * dy
}

fn read_areas(display: &Display) -> Vec<Rect> {
    let randr = display
        .connection
        .randr_get_monitors(display.root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| {
            reply
                .monitors
                .iter()
                .map(|monitor| Rect {
                    x: i32::from(monitor.x),
                    y: i32::from(monitor.y),
                    width: i32::from(monitor.width),
                    height: i32::from(monitor.height),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !randr.is_empty() {
        return randr;
    }
    let screen = &display.connection.setup().roots[display.screen];
    vec![Rect {
        x: 0,
        y: 0,
        width: i32::from(screen.width_in_pixels),
        height: i32::from(screen.height_in_pixels),
    }]
}

fn read_scale(display: &Display) -> Scale {
    let dpi = new_from_resource_manager(&display.connection)
        .ok()
        .flatten()
        .and_then(|database| database.get_value::<f64>(DPI_RESOURCE, DPI_CLASS).ok())
        .flatten()
        .map_or(BASE_DPI, clamp_dpi);
    Scale(dpi)
}

fn read_compositing(display: &Display, compositor: Atom) -> bool {
    display
        .connection
        .get_selection_owner(compositor)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.owner != x11rb::NONE)
}

impl Desktop {
    pub fn new(display: Arc<Display>) -> Result<Self, DisplayError> {
        let connection = &display.connection;
        let name = format!("_NET_WM_CM_S{}", display.screen);
        let compositor = connection
            .intern_atom(false, name.as_bytes())?
            .reply()?
            .atom;
        connection.change_window_attributes(
            display.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let _ = connection
            .randr_query_version(RANDR_VERSION.0, RANDR_VERSION.1)
            .map(|cookie| cookie.reply());
        let _ = connection.randr_select_input(display.root, NotifyMask::SCREEN_CHANGE);
        let _ = connection
            .xfixes_query_version(XFIXES_VERSION.0, XFIXES_VERSION.1)
            .map(|cookie| cookie.reply());
        let _ = connection.xfixes_select_selection_input(
            display.root,
            compositor,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        );
        connection.flush()?;
        let view = View {
            areas: read_areas(&display),
            scale: read_scale(&display),
            compositing: read_compositing(&display, compositor),
        };
        Ok(Self {
            display,
            compositor,
            view: Arc::new(RwLock::new(view)),
        })
    }

    pub(crate) fn observe(&self, event: &Event) {
        match event {
            Event::PropertyNotify(change)
                if change.window == self.display.root
                    && change.atom == u32::from(AtomEnum::RESOURCE_MANAGER) =>
            {
                self.update(|view| view.scale = read_scale(&self.display));
            }
            Event::RandrScreenChangeNotify(_) => {
                self.update(|view| view.areas = read_areas(&self.display));
            }
            Event::XfixesSelectionNotify(_) => {
                self.update(|view| {
                    view.compositing = read_compositing(&self.display, self.compositor);
                });
            }
            _ => {}
        }
    }

    fn update(&self, change: impl FnOnce(&mut View)) {
        change(&mut self.view.write().unwrap_or_else(PoisonError::into_inner));
    }

    fn read<T>(&self, pick: impl FnOnce(&View) -> T) -> T {
        pick(&self.view.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn compositing(&self) -> bool {
        self.read(|view| view.compositing)
    }

    pub fn work_areas(&self) -> Vec<Rect> {
        self.read(|view| view.areas.clone())
    }

    pub fn nearest(&self, target: Rect) -> Option<Monitor> {
        let (x, y) = (target.x + target.width / 2, target.y + target.height / 2);
        self.read(|view| {
            view.areas
                .iter()
                .min_by_key(|area| distance_to(**area, x, y))
                .map(|work| Monitor {
                    work: *work,
                    scale: view.scale,
                })
        })
    }
}
