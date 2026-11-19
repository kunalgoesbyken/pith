#[cfg(target_os = "linux")]
mod spike {
    use std::error::Error;
    use std::time::Duration;

    use serde_json::json;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_registry, wl_seat};
    use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
    use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
    use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::{
        Event, ZwpInputMethodV2,
    };

    const SUGGESTION: &str = "ghost";
    const PREEDIT_HOLD: Duration = Duration::from_millis(1500);
    const NO_CURSOR: i32 = -1;

    #[derive(Clone, Default)]
    struct Content {
        active: bool,
        surrounding: Option<(String, u32)>,
    }

    #[derive(Default)]
    struct State {
        pending: Content,
        current: Content,
        serial: u32,
        fresh: bool,
        unavailable: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    delegate_noop!(State: ignore wl_seat::WlSeat);
    delegate_noop!(State: ZwpInputMethodManagerV2);

    impl Dispatch<ZwpInputMethodV2, ()> for State {
        fn event(
            state: &mut Self,
            _: &ZwpInputMethodV2,
            event: Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                Event::Activate => state.pending.active = true,
                Event::Deactivate => state.pending = Content::default(),
                Event::SurroundingText { text, cursor, .. } => {
                    state.pending.surrounding = Some((text, cursor));
                }
                Event::Done => {
                    state.serial += 1;
                    state.current = state.pending.clone();
                    state.fresh = true;
                }
                Event::Unavailable => state.unavailable = true,
                _ => {}
            }
        }
    }

    fn report(event: &str, detail: serde_json::Value) {
        println!("{}", json!({ "event": event, "detail": detail }));
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
        let handle = queue.handle();
        let seat: wl_seat::WlSeat = globals.bind(&handle, 1..=1, ())?;
        let manager: ZwpInputMethodManagerV2 = globals.bind(&handle, 1..=1, ())?;
        let method = manager.get_input_method(&seat, &handle, ());
        let mut state = State::default();
        let mut acted = false;
        loop {
            queue.blocking_dispatch(&mut state)?;
            if state.unavailable {
                return Err("another input method already owns the seat".into());
            }
            if !state.fresh {
                continue;
            }
            state.fresh = false;
            report("state", json!(state.current.surrounding));
            if !state.current.active {
                acted = false;
                continue;
            }
            if acted {
                continue;
            }
            acted = true;
            report("activated", json!(state.current.surrounding));
            let length = i32::try_from(SUGGESTION.len())?;
            method.set_preedit_string(SUGGESTION.to_string(), 0, length);
            method.commit(state.serial);
            queue.roundtrip(&mut state)?;
            report("preedit", json!(SUGGESTION));
            std::thread::sleep(PREEDIT_HOLD);
            queue.roundtrip(&mut state)?;
            method.set_preedit_string(String::new(), NO_CURSOR, NO_CURSOR);
            method.commit_string(SUGGESTION.to_string());
            method.commit(state.serial);
            queue.roundtrip(&mut state)?;
            report("committed", json!(SUGGESTION));
        }
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    match spike::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
