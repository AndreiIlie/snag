use crate::run::{Choice, OverlayWindow};
use snag_core::{
    geometry::{Point, ScaleFactor},
    overlay::{Cursor, Key, Overlay},
    platform::Screenshot,
    render::{Backdrop, pack_frame, render_overlay},
    selection::Handle,
    toolbar::Action,
    view::View,
};
use softbuffer::{Context, Surface};
use std::{error::Error, num::NonZeroU32, rc::Rc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, OwnedDisplayHandle},
    keyboard::{Key as LogicalKey, ModifiersState, NamedKey},
    platform::wayland::WindowAttributesExtWayland,
    window::{CursorIcon, Fullscreen, Window, WindowId},
};

pub struct FullscreenOverlay;

impl OverlayWindow for FullscreenOverlay {
    fn choose(self, shot: &Screenshot) -> Result<Choice, Box<dyn Error>> {
        let capture = (shot.image.width(), shot.image.height());
        let mut app = App {
            capture,
            size: PhysicalSize::new(0, 0),
            model: Overlay::new(View::fit(capture, capture).unwrap(), ScaleFactor::ONE),
            backdrop: Backdrop::new(&shot.image),
            surface: None,
            modifiers: ModifiersState::empty(),
            cursor: None,
            result: None,
        };
        EventLoop::new()?.run_app(&mut app)?;
        app.result.ok_or("overlay closed without an action")?
    }
}

struct App<'a> {
    capture: (u32, u32),
    size: PhysicalSize<u32>,
    model: Overlay,
    backdrop: Backdrop<'a>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    modifiers: ModifiersState,
    cursor: Option<Cursor>,
    result: Option<Result<Choice, Box<dyn Error>>>,
}

impl App<'_> {
    fn open(&mut self, events: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let attributes = Window::default_attributes()
            .with_title("snag")
            .with_name("snag", "snag")
            .with_decorations(false)
            .with_fullscreen(Some(Fullscreen::Borderless(None)));
        let window = Rc::new(events.create_window(attributes)?);
        let context = Context::new(events.owned_display_handle())?;
        self.surface = Some(Surface::new(&context, window.clone())?);
        self.resize(window.inner_size(), window.scale_factor());
        window.set_cursor(CursorIcon::Crosshair);
        self.cursor = Some(Cursor::Crosshair);
        window.request_redraw();
        Ok(())
    }

    fn resize(&mut self, size: PhysicalSize<u32>, scale: f64) -> bool {
        let was_empty = self.size.width == 0 || self.size.height == 0;
        self.size = size;
        let Some(view) = View::fit(self.capture, size.into()) else {
            return false;
        };
        self.model
            .set_view(view, ScaleFactor::new(scale).unwrap_or(ScaleFactor::ONE))
            || was_empty
    }

    fn redraw(&mut self, window: &Window) -> Result<(), Box<dyn Error>> {
        let size = self.size;
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let surface = self.surface.as_mut().unwrap();
        surface.resize(width, height)?;
        let mut buffer = surface.buffer_mut()?;
        render_overlay(
            &mut self.backdrop,
            &self.model,
            bytemuck::cast_slice_mut(&mut buffer),
        )?;
        pack_frame(&mut buffer);
        window.pre_present_notify();
        buffer.present()?;
        Ok(())
    }

    fn finish(&mut self, events: &ActiveEventLoop, result: Result<Choice, Box<dyn Error>>) {
        self.result = Some(result);
        events.exit();
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.surface.is_none()
            && let Err(e) = self.open(events)
        {
            self.finish(events, Err(e));
        }
    }

    fn window_event(&mut self, events: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(surface) = &self.surface else { return };
        if self.result.is_some() {
            return;
        }
        let window = surface.window().clone();
        let mut action = None;
        let redraw = match event {
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw(&window) {
                    self.finish(events, Err(e));
                }
                false
            }
            WindowEvent::Resized(size) => self.resize(size, window.scale_factor()),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.resize(window.inner_size(), scale_factor)
            }
            WindowEvent::CursorMoved { position, .. } => self.model.pointer_moved(Point {
                x: position.x,
                y: position.y,
            }),
            WindowEvent::CursorLeft { .. } => {
                self.model.pointer_left();
                false
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => match state {
                ElementState::Pressed => self.model.press(),
                ElementState::Released => {
                    let (changed, button) = self.model.release();
                    action = button;
                    changed
                }
            },
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                false
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state.is_pressed() && !event.repeat => {
                let key = match event.logical_key.as_ref() {
                    LogicalKey::Named(NamedKey::Enter) => Some(Key::Enter),
                    LogicalKey::Named(NamedKey::Escape) => Some(Key::Escape),
                    LogicalKey::Character(c)
                        if self.modifiers == ModifiersState::CONTROL
                            && c.eq_ignore_ascii_case("c") =>
                    {
                        Some(Key::Copy)
                    }
                    LogicalKey::Character(c)
                        if self.modifiers == ModifiersState::CONTROL
                            && c.eq_ignore_ascii_case("s") =>
                    {
                        Some(Key::Save)
                    }
                    _ => None,
                };
                action = key.map(Key::action);
                false
            }
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::empty();
                self.model.reset_pointer()
            }
            WindowEvent::CloseRequested => {
                action = Some(Action::Cancel);
                false
            }
            _ => return,
        };
        if let Some(action) = action {
            self.finish(events, Ok((action, self.model.crop())));
            return;
        }
        let cursor = self.model.cursor();
        if self.cursor != Some(cursor) {
            let icon = match cursor {
                Cursor::Arrow => CursorIcon::Default,
                Cursor::Move => CursorIcon::Move,
                Cursor::Crosshair => CursorIcon::Crosshair,
                Cursor::Resize(Handle::TopLeft | Handle::BottomRight) => CursorIcon::NwseResize,
                Cursor::Resize(Handle::TopRight | Handle::BottomLeft) => CursorIcon::NeswResize,
                Cursor::Resize(Handle::Left | Handle::Right) => CursorIcon::EwResize,
                Cursor::Resize(Handle::Top | Handle::Bottom) => CursorIcon::NsResize,
            };
            window.set_cursor(icon);
            self.cursor = Some(cursor);
        }
        if redraw {
            window.request_redraw();
        }
    }
}
