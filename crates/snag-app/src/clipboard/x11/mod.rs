mod transfers;

use super::{Owner, Png, Result};
use rustix::event::{PollFd, PollFlags, poll};
use std::time::Instant;
use transfers::{Property as TransferKey, Transfers, convert_pairs};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::{Connection, RequestConnection},
    errors::ReplyError,
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD, TARGETS, TIMESTAMP, MULTIPLE, ATOM_PAIR, INCR, PNG: b"image/png",
    }
}

struct X11Owner {
    conn: RustConnection,
    atoms: Atoms,
    window: Window,
    time: Timestamp,
    png: Png,
    transfers: Transfers,
}

pub(super) fn claim(png: Png) -> Result<Box<dyn Owner>> {
    let (conn, screen) = x11rb::connect(None)?;
    let window = conn.generate_id()?;
    let root = conn.setup().roots[screen].root;
    conn.create_window(
        0,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    let atoms = Atoms::new(&conn)?.reply()?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atoms.TIMESTAMP,
        AtomEnum::INTEGER,
        &[],
    )?
    .check()?;
    let time = loop {
        if let Event::PropertyNotify(event) = conn.wait_for_event()?
            && event.window == window
            && event.atom == atoms.TIMESTAMP
        {
            break event.time;
        }
    };
    conn.set_selection_owner(window, atoms.CLIPBOARD, time)?
        .check()?;
    if conn.get_selection_owner(atoms.CLIPBOARD)?.reply()?.owner != window {
        return Err("could not acquire the X11 CLIPBOARD selection".into());
    }
    // ChangeProperty has a 24-byte header, plus a length word for BIG-REQUESTS.
    let request_limit = conn.maximum_request_bytes();
    let header = if request_limit > usize::from(u16::MAX) * 4 {
        28
    } else {
        24
    };
    let transfers = Transfers::new(png.len(), request_limit - header);
    Ok(Box::new(X11Owner {
        conn,
        atoms,
        window,
        time,
        png,
        transfers,
    }))
}

// A failed conversion belongs to the requestor. A broken connection ends ownership.
fn client_result(
    result: std::result::Result<bool, ReplyError>,
) -> std::result::Result<bool, ReplyError> {
    match result {
        Err(ReplyError::X11Error(_)) => Ok(false),
        other => other,
    }
}

impl X11Owner {
    fn convert(
        &mut self,
        requestor: Window,
        target: Atom,
        property: Atom,
    ) -> std::result::Result<bool, ReplyError> {
        let atoms = self.atoms;
        let key = (requestor, property);
        if self.transfers.active(key) {
            return Ok(false);
        }
        if target == atoms.TARGETS {
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    AtomEnum::ATOM,
                    &[atoms.TARGETS, atoms.TIMESTAMP, atoms.MULTIPLE, atoms.PNG],
                )?
                .check()?;
        } else if target == atoms.TIMESTAMP {
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    AtomEnum::INTEGER,
                    &[self.time],
                )?
                .check()?;
        } else if target == atoms.PNG {
            if self.transfers.incremental() {
                self.conn
                    .change_window_attributes(
                        requestor,
                        &ChangeWindowAttributesAux::new()
                            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                    )?
                    .check()?;
                self.conn
                    .change_property32(
                        PropMode::REPLACE,
                        requestor,
                        property,
                        atoms.INCR,
                        &[u32::try_from(self.png.len()).unwrap_or(u32::MAX)],
                    )?
                    .check()?;
                self.transfers.start(key, Instant::now());
            } else {
                self.conn
                    .change_property8(PropMode::REPLACE, requestor, property, atoms.PNG, &self.png)?
                    .check()?;
            }
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    fn multiple(
        &mut self,
        requestor: Window,
        property: Atom,
    ) -> std::result::Result<bool, ReplyError> {
        let reply = self
            .conn
            .get_property(
                false,
                requestor,
                property,
                self.atoms.ATOM_PAIR,
                0,
                (self.transfers.limit / 4) as u32,
            )?
            .reply()?;
        if reply.type_ != self.atoms.ATOM_PAIR
            || reply.format != 32
            || reply.bytes_after != 0
            || reply.value_len % 2 != 0
        {
            return Ok(false);
        }
        let mut pairs: Vec<_> = reply.value32().expect("32-bit property").collect();
        convert_pairs(&mut pairs, property, |target, dest| {
            client_result(self.convert(requestor, target, dest))
        })?;
        self.conn
            .change_property32(
                PropMode::REPLACE,
                requestor,
                property,
                self.atoms.ATOM_PAIR,
                &pairs,
            )?
            .check()?;
        Ok(true)
    }

    fn request(&mut self, req: SelectionRequestEvent) -> std::result::Result<(), ReplyError> {
        let property = if req.property == NONE && req.target != self.atoms.MULTIPLE {
            req.target
        } else {
            req.property
        };
        let valid = self.transfers.owning
            && req.owner == self.window
            && req.selection == self.atoms.CLIPBOARD
            && property != NONE
            && (req.time == CURRENT_TIME || req.time.wrapping_sub(self.time) as i32 >= 0);
        let success = if !valid {
            false
        } else if req.target == self.atoms.MULTIPLE {
            client_result(self.multiple(req.requestor, property))?
        } else {
            client_result(self.convert(req.requestor, req.target, property))?
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: req.time,
            requestor: req.requestor,
            selection: req.selection,
            target: req.target,
            property: if success { property } else { NONE },
        };
        self.conn
            .send_event(false, req.requestor, EventMask::NO_EVENT, notify)?
            .check()
    }

    fn send_chunk(&mut self, key: TransferKey) -> std::result::Result<(), ReplyError> {
        let Some(range) = self.transfers.chunk(key) else {
            return Ok(());
        };
        let result = self
            .conn
            .change_property8(
                PropMode::REPLACE,
                key.0,
                key.1,
                self.atoms.PNG,
                &self.png[range.clone()],
            )?
            .check();
        if client_result(result.map(|()| true))? {
            self.transfers.sent(key, range, Instant::now());
        } else {
            self.transfers.cancel(key);
        }
        self.unwatch_if_idle(key.0)
    }

    fn unwatch_if_idle(&self, window: Window) -> std::result::Result<(), ReplyError> {
        if !self.transfers.watching(window) {
            self.conn
                .change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
                )?
                .check()?;
        }
        Ok(())
    }

    fn event_before(&self, deadline: Instant) -> Result<Option<Event>> {
        if let Some(event) = self.conn.poll_for_event()? {
            return Ok(Some(event));
        }
        let mut fds = [PollFd::new(self.conn.stream(), PollFlags::IN)];
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .try_into()?;
        match poll(&mut fds, Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

impl Owner for X11Owner {
    fn wait_until_lost(mut self: Box<Self>) -> Result<()> {
        while self.transfers.serving() {
            let event = if let Some(deadline) = self.transfers.earliest_deadline() {
                let now = Instant::now();
                if deadline <= now {
                    for window in self.transfers.expire(now) {
                        client_result(self.unwatch_if_idle(window).map(|()| true))?;
                    }
                    continue;
                }
                let Some(event) = self.event_before(deadline)? else {
                    continue;
                };
                event
            } else {
                self.conn.wait_for_event()?
            };
            let result = match event {
                Event::SelectionRequest(req) => self.request(req),
                Event::SelectionClear(clear)
                    if clear.owner == self.window && clear.selection == self.atoms.CLIPBOARD =>
                {
                    self.transfers.owning = false;
                    Ok(())
                }
                Event::PropertyNotify(event) if event.state == Property::DELETE => {
                    self.send_chunk((event.window, event.atom))
                }
                Event::DestroyNotify(event) => {
                    self.transfers.forget_window(event.window);
                    Ok(())
                }
                _ => Ok(()),
            };
            client_result(result.map(|()| true))?;
        }
        Ok(())
    }
}
