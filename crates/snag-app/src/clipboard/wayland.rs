use super::{Owner, PNG_MIME, Png, Result, TRANSFER_TIMEOUT};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use std::{
    fs::File,
    io::{self, Write},
    os::fd::OwnedFd,
    sync::mpsc,
    thread,
    time::Instant,
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

struct Writers {
    active: mpsc::Sender<()>,
    finished: mpsc::Receiver<()>,
}

impl Writers {
    fn new() -> Self {
        let (active, finished) = mpsc::channel();
        Self { active, finished }
    }

    fn write(&self, fd: OwnedFd, png: Png) {
        let active = self.active.clone();
        // Each paste runs independently. The last writer disconnects the channel during drain.
        thread::spawn(move || {
            let _ = send_png(fd, &png);
            drop(active);
        });
    }

    fn drain(self, deadline: Instant) {
        drop(self.active);
        let _ = self
            .finished
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

struct State {
    png: Png,
    writers: Writers,
    drain_deadline: Option<Instant>,
}

impl State {
    fn lost(&mut self) {
        self.drain_deadline
            .get_or_insert_with(|| Instant::now() + TRANSFER_TIMEOUT);
    }
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
    let mut state = State {
        png,
        writers: Writers::new(),
        drain_deadline: None,
    };
    queue.roundtrip(&mut state)?;
    if let Some(deadline) = state.drain_deadline {
        state.writers.drain(deadline);
        return Err("Wayland clipboard ownership lost during claim".into());
    }
    Ok(Box::new(WaylandOwner { queue, state }))
}

impl Owner for WaylandOwner {
    fn wait_until_lost(mut self: Box<Self>) -> Result<()> {
        loop {
            if let Some(deadline) = self.state.drain_deadline {
                self.state.writers.drain(deadline);
                return Ok(());
            }
            self.queue.blocking_dispatch(&mut self.state)?;
        }
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
                    $device::Event::Finished => state.lost(),
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
                    $source::Event::Send { mime_type, fd } if mime_type == PNG_MIME && state.drain_deadline.is_none() => {
                        state.writers.write(fd, state.png.clone());
                    }
                    $source::Event::Cancelled => state.lost(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, os::unix::net::UnixStream, time::Duration};

    // Fill the socket before starting the paste so the writer needs the reader to make progress.
    fn pending_paste() -> (Writers, UnixStream, usize, Png) {
        let (mut socket, reader) = UnixStream::pair().unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut buffered = 0;
        loop {
            match socket.write(&[0; 8192]) {
                Ok(n) => buffered += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        let png = Png::new(vec![0xab; buffered + 8192]);
        let writers = Writers::new();
        writers.write(socket.into(), png.clone());
        (writers, reader, buffered, png)
    }

    #[test]
    fn waits_for_a_paste_to_finish() {
        let (writers, mut reader, buffered, png) = pending_paste();
        let (drained, wait) = mpsc::channel();
        let reader = thread::spawn(move || {
            assert_eq!(
                wait.recv_timeout(Duration::from_millis(50)),
                Err(mpsc::RecvTimeoutError::Timeout)
            );
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let start = Instant::now();
        writers.drain(start + TRANSFER_TIMEOUT);
        let _ = drained.send(());
        assert!(start.elapsed() < TRANSFER_TIMEOUT / 2);
        assert_eq!(&reader.join().unwrap()[buffered..], png.as_slice());
    }

    #[test]
    fn stops_waiting_for_a_stalled_reader_at_the_deadline() {
        let (writers, reader, _, _) = pending_paste();
        let timeout = Duration::from_millis(50);
        let (drained, wait) = mpsc::channel();
        thread::spawn(move || {
            let start = Instant::now();
            writers.drain(start + timeout);
            drained.send(start.elapsed()).unwrap();
        });
        assert!(wait.recv_timeout(Duration::from_secs(2)).unwrap() >= timeout);
        drop(reader);
    }

    #[test]
    fn drains_immediately_without_a_writer() {
        let start = Instant::now();
        Writers::new().drain(start + TRANSFER_TIMEOUT);
        assert!(start.elapsed() < TRANSFER_TIMEOUT / 2);
    }
}
