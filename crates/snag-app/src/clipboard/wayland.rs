use super::{Owner, PNG_MIME, Png, Result};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use std::{
    fs::File,
    io::{self, Write},
    os::fd::OwnedFd,
    thread,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat::WlSeat},
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1 as ext_device, ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::ExtDataControlOfferV1, ext_data_control_source_v1 as ext_source,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as wlr_device,
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, zwlr_data_control_source_v1 as wlr_source,
};

fn send_png(fd: OwnedFd, png: &[u8]) -> io::Result<()> {
    // Receivers can supply a nonblocking pipe. This writer runs on its own thread.
    let flags = fcntl_getfl(&fd)?;
    if flags.contains(OFlags::NONBLOCK) {
        fcntl_setfl(&fd, flags - OFlags::NONBLOCK)?;
    }
    File::from(fd).write_all(png)
}

struct State {
    png: Png,
    lost: bool,
}

struct WaylandOwner {
    queue: EventQueue<State>,
    state: State,
}

pub(super) fn claim(png: Png) -> Result<Box<dyn Owner>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
    if let Ok(manager) = globals.bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ()) {
        let device = manager.get_data_device(&seat, &qh, ());
        let source = manager.create_data_source(&qh, ());
        source.offer(PNG_MIME.into());
        device.set_selection(Some(&source));
    } else {
        let manager: ZwlrDataControlManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|e| format!("Wayland data-control unavailable: {e}"))?;
        let device = manager.get_data_device(&seat, &qh, ());
        let source = manager.create_data_source(&qh, ());
        source.offer(PNG_MIME.into());
        device.set_selection(Some(&source));
    }
    let mut state = State { png, lost: false };
    queue.roundtrip(&mut state)?;
    if state.lost {
        return Err("Wayland clipboard ownership lost during claim".into());
    }
    Ok(Box::new(WaylandOwner { queue, state }))
}

impl Owner for WaylandOwner {
    fn wait_until_lost(mut self: Box<Self>) -> Result<()> {
        while !self.state.lost {
            self.queue.blocking_dispatch(&mut self.state)?;
        }
        Ok(())
    }
}

delegate_noop!(State: ignore WlSeat);
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

// The standard and wlroots protocols use the same events with distinct generated types.
macro_rules! dispatch_data_control {
    ($manager:ty, $device:ident::$device_ty:ident, $source:ident::$source_ty:ident, $offer:ty) => {
        delegate_noop!(State: ignore $manager);
        delegate_noop!(State: ignore $offer);

        impl Dispatch<$device::$device_ty, ()> for State {
            fn event(state: &mut Self, _: &$device::$device_ty, event: $device::Event, _: &(),
                _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $device::Event::Selection { id: Some(offer) }
                    | $device::Event::PrimarySelection { id: Some(offer) } => offer.destroy(),
                    $device::Event::Finished => state.lost = true,
                    _ => (),
                }
            }
            event_created_child!(State, $device::$device_ty, [
                $device::EVT_DATA_OFFER_OPCODE => ($offer, ())
            ]);
        }

        impl Dispatch<$source::$source_ty, ()> for State {
            fn event(state: &mut Self, _: &$source::$source_ty, event: $source::Event, _: &(),
                _: &Connection, _: &QueueHandle<Self>) {
                match event {
                    $source::Event::Send { mime_type, fd } if mime_type == PNG_MIME => {
                        // A slow or abandoned reader must not block cancellation or another paste.
                        let png = state.png.clone();
                        thread::spawn(move || { let _ = send_png(fd, &png); });
                    }
                    $source::Event::Cancelled => state.lost = true,
                    _ => (),
                }
            }
        }
    };
}

dispatch_data_control!(
    ExtDataControlManagerV1,
    ext_device::ExtDataControlDeviceV1,
    ext_source::ExtDataControlSourceV1,
    ExtDataControlOfferV1
);
dispatch_data_control!(
    ZwlrDataControlManagerV1,
    wlr_device::ZwlrDataControlDeviceV1,
    wlr_source::ZwlrDataControlSourceV1,
    ZwlrDataControlOfferV1
);
