#![allow(unused_imports)]

use {
    glutin::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
    },
    reclutch::{
        display::{
            self, Color, CommandGroup, DisplayClip, DisplayCommand, DisplayItem, Filter, FontInfo,
            GraphicsDisplay, GraphicsDisplayItem, GraphicsDisplayPaint, GraphicsDisplayStroke,
            Point, Rect, ResourceData, ResourceDescriptor, ResourceReference, Size, StyleColor,
            TextDisplayItem, Vector,
        },
        event::{merge::Merge, RcEventListener, RcEventQueue},
        prelude::*,
        Widget, WidgetChildren,
    },
};

#[derive(Clone)]
struct ConsumableEvent<T>(std::rc::Rc<std::cell::RefCell<Option<T>>>);

impl<T> ConsumableEvent<T> {
    fn new(val: T) -> Self {
        ConsumableEvent(std::rc::Rc::new(std::cell::RefCell::new(Some(val))))
    }

    fn with<P: FnMut(&T) -> bool>(&self, mut pred: P) -> Option<T> {
        if self.0.borrow().is_some() {
            if pred(self.0.borrow().as_ref().unwrap()) {
                return self.0.replace(None);
            }
        }

        None
    }
}

#[derive(Clone)]
enum GlobalEvent {
    MouseClick(ConsumableEvent<Point>),
    MouseRelease(Point),
    MouseMove(Point),
    WindowResize,
}

struct Globals {
    hidpi_factor: f64,
    cursor: Point,
    size: Size,
}

#[derive(Debug, Clone, Copy)]
enum TitlebarEvent {
    BeginClick(Point),
    Move(Vector),
    EndClick,
}

#[derive(WidgetChildren)]
struct Titlebar {
    pub move_event: RcEventQueue<TitlebarEvent>,
    position: Point,
    cursor_anchor: Option<Point>,
    global_listener: RcEventListener<GlobalEvent>,
    command_group: CommandGroup,
    width: f32,
    text: String,
    font: FontInfo,
    font_resource: Option<ResourceReference>,
}

impl Titlebar {
    fn new(
        position: Point,
        width: f32,
        text: String,
        global: &mut RcEventQueue<GlobalEvent>,
    ) -> Self {
        Titlebar {
            move_event: RcEventQueue::new(),
            position,
            cursor_anchor: None,
            global_listener: global.listen(),
            command_group: CommandGroup::new(),
            width,
            text,
            font: FontInfo::from_name("Segoe UI", &["SF Display", "Arial"]).unwrap(),
            font_resource: None,
        }
    }

    fn set_position(&mut self, position: Point) {
        self.position = position;
        self.command_group.repaint();
    }
}

impl Widget for Titlebar {
    type Aux = Globals;

    fn bounds(&self) -> Rect {
        Rect::new(self.position, Size::new(self.width, 30.0))
    }

    fn update(&mut self, _aux: &mut Globals) {
        for event in self.global_listener.peek() {
            match event {
                GlobalEvent::MouseClick(click) => {
                    if let Some(ref position) =
                        click.with(|pos| self.bounds().contains(pos.clone()))
                    {
                        self.cursor_anchor = Some(position.clone());
                        self.move_event
                            .push(TitlebarEvent::BeginClick(position.clone()));
                    }
                }
                GlobalEvent::MouseRelease(_) => {
                    if self.cursor_anchor.is_some() {
                        self.cursor_anchor = None;
                        self.move_event.push(TitlebarEvent::EndClick);
                    }
                }
                GlobalEvent::MouseMove(pos) => {
                    if let Some(ref cursor_anchor) = self.cursor_anchor {
                        self.move_event
                            .push(TitlebarEvent::Move(pos - *cursor_anchor));
                    }
                }
                _ => (),
            }
        }
    }

    fn draw(&mut self, display: &mut dyn GraphicsDisplay) {
        if self.font_resource.is_none() {
            self.font_resource = display
                .new_resource(ResourceDescriptor::Font(ResourceData::Data(
                    self.font.data().unwrap(),
                )))
                .ok();
        }

        let bounds = self.bounds();

        self.command_group.push(
            display,
            &[
                DisplayCommand::BackdropFilter(
                    DisplayClip::Rectangle {
                        rect: bounds.clone(),
                        antialias: true,
                    },
                    Filter::Blur(10.0, 10.0),
                ),
                DisplayCommand::Item(DisplayItem::Graphics(GraphicsDisplayItem::Rectangle {
                    rect: bounds.clone(),
                    paint: GraphicsDisplayPaint::Fill(StyleColor::Color(Color::new(
                        1.0, 1.0, 1.0, 0.6,
                    ))),
                })),
                DisplayCommand::Item(DisplayItem::Graphics(GraphicsDisplayItem::Line {
                    a: Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height),
                    b: Point::new(
                        bounds.origin.x + bounds.size.width,
                        bounds.origin.y + bounds.size.height,
                    ),
                    stroke: GraphicsDisplayStroke {
                        thickness: 1.0,
                        antialias: false,
                        ..Default::default()
                    },
                })),
                DisplayCommand::Item(DisplayItem::Text(TextDisplayItem {
                    text: self.text.clone(),
                    font: self.font_resource.as_ref().unwrap().clone(),
                    font_info: self.font.clone(),
                    size: 22.0,
                    bottom_left: bounds.origin + Size::new(5.0, 22.0),
                    color: StyleColor::Color(Color::new(0.0, 0.0, 0.0, 1.0)),
                })),
            ],
        );
    }
}

#[derive(WidgetChildren)]
struct Panel {
    pub on_click: RcEventQueue<*const Panel>,
    #[widget_child]
    titlebar: Titlebar,
    position_anchor: Option<Point>,
    position: Point,
    size: Size,
    global_listener: RcEventListener<GlobalEvent>,
    titlebar_move_listener: RcEventListener<TitlebarEvent>,
    command_group: CommandGroup,
    image_path: std::path::PathBuf,
    image: Option<ResourceReference>,
}

impl Panel {
    fn new(
        position: Point,
        size: Size,
        text: String,
        image_path: std::path::PathBuf,
        global: &mut RcEventQueue<GlobalEvent>,
    ) -> Self {
        let titlebar = Titlebar::new(position.clone(), size.width - 1.0, text, global);
        let titlebar_move_listener = titlebar.move_event.listen();

        Panel {
            on_click: RcEventQueue::new(),
            titlebar,
            position_anchor: None,
            position,
            size,
            global_listener: global.listen(),
            titlebar_move_listener,
            command_group: CommandGroup::new(),
            image_path,
            image: None,
        }
    }

    fn fit_in_window(&mut self, size: &Size) {
        let window_rect = Rect::new(Point::default(), size.clone());
        let bounds = self.bounds();

        let vert = if bounds.min_y() < window_rect.min_y() {
            window_rect.min_y() - bounds.min_y()
        } else if bounds.max_y() > window_rect.max_y() {
            window_rect.max_y() - bounds.max_y()
        } else {
            0.0
        };

        let horiz = if bounds.min_x() < window_rect.min_x() {
            window_rect.min_x() - bounds.min_x()
        } else if bounds.max_x() > window_rect.max_x() {
            window_rect.max_x() - bounds.max_x()
        } else {
            0.0
        };

        self.position += Vector::new(horiz, vert);
    }
}

impl Widget for Panel {
    type Aux = Globals;

    fn bounds(&self) -> Rect {
        Rect::new(self.position, self.size)
    }

    fn update(&mut self, aux: &mut Globals) {
        for child in self.children_mut() {
            child.update(aux);
        }

        for event in self.titlebar_move_listener.peek() {
            match event {
                TitlebarEvent::BeginClick(_) => {
                    self.position_anchor = Some(self.position);
                    self.on_click.push(self as _);
                }
                TitlebarEvent::Move(delta) => {
                    if let Some(position_anchor) = self.position_anchor {
                        self.position = position_anchor + delta;

                        self.fit_in_window(&aux.size);

                        self.titlebar.set_position(self.position.clone());
                        self.command_group.repaint();
                    }
                }
                TitlebarEvent::EndClick => {
                    self.position_anchor = None;
                }
            }
        }

        for event in self.global_listener.peek() {
            match event {
                GlobalEvent::MouseClick(click) => {
                    if let Some(_) = click.with(|pos| self.bounds().contains(pos.clone())) {
                        self.on_click.push(self as _);
                    }
                }
                GlobalEvent::WindowResize => {
                    self.fit_in_window(&aux.size);

                    self.titlebar.set_position(self.position.clone());
                    self.command_group.repaint();
                }
                _ => (),
            }
        }
    }

    fn draw(&mut self, display: &mut dyn GraphicsDisplay) {
        if self.image.is_none() {
            self.image = display
                .new_resource(ResourceDescriptor::Image(ResourceData::File(
                    /*,*/
                    self.image_path.clone(),
                )))
                .ok();
        }

        let bounds = self.bounds();

        self.command_group.push(
            display,
            &[
                DisplayCommand::BackdropFilter(
                    DisplayClip::Rectangle {
                        rect: bounds.clone(),
                        antialias: true,
                    },
                    Filter::Blur(5.0, 5.0),
                ),
                DisplayCommand::Item(DisplayItem::Graphics(GraphicsDisplayItem::Rectangle {
                    rect: bounds.clone(),
                    paint: GraphicsDisplayPaint::Fill(StyleColor::Color(Color::new(
                        0.9, 0.9, 0.9, 0.5,
                    ))),
                })),
                DisplayCommand::Item(DisplayItem::Graphics(GraphicsDisplayItem::Image {
                    src: None,
                    dst: bounds.clone(),
                    resource: self.image.clone().unwrap(),
                })),
                DisplayCommand::Item(DisplayItem::Graphics(GraphicsDisplayItem::Rectangle {
                    rect: bounds.inflate(0.0, 1.0), /*.round_out()*/
                    paint: GraphicsDisplayPaint::Stroke(GraphicsDisplayStroke {
                        color: StyleColor::Color(Color::new(0.0, 0.0, 0.0, 1.0)),
                        thickness: 1.0,
                        antialias: false,
                        ..Default::default()
                    }),
                })),
            ],
        );

        for child in self.children_mut() {
            child.draw(display);
        }
    }
}

struct PanelContainer {
    panels: Vec<(Panel, RcEventListener<*const Panel>)>,
}

impl PanelContainer {
    fn new() -> Self {
        PanelContainer { panels: Vec::new() }
    }

    fn add_panel(&mut self, panel: Panel) {
        let on_click_listener = panel.on_click.listen();
        self.panels.push((panel, on_click_listener));
    }
}

impl WidgetChildren<Globals> for PanelContainer {
    fn children(&self) -> Vec<&dyn WidgetChildren<Globals>> {
        self.panels.iter().map(|(ref p, _)| p as _).collect()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn WidgetChildren<Globals>> {
        self.panels
            .iter_mut()
            .map(|(ref mut p, _)| p as _)
            .collect()
    }
}

impl Widget for PanelContainer {
    type Aux = Globals;

    fn update(&mut self, globals: &mut Globals) {
        // propagate back to front so that panels rendered front-most get events first.
        for child in self.children_mut().iter_mut().rev() {
            child.update(globals);
        }

        {
            // collect all the panel events into a single vec
            let mut panel_events = Vec::new();
            for panel in &self.panels {
                panel.1.extend_other(&mut panel_events);
            }

            for event in panel_events {
                if let Some(panel_idx) = self
                    .panels
                    .iter()
                    .position(|(ref p, _)| p as *const Panel == event)
                {
                    let last = self.panels.len() - 1;
                    self.panels.swap(panel_idx, last);
                }
            }
        }
    }

    fn draw(&mut self, display: &mut dyn GraphicsDisplay) {
        for child in self.children_mut() {
            child.draw(display);
        }
    }
}

#[cfg(feature = "skia")]
fn main() {
    let window_size = (500u32, 500u32);

    let event_loop = EventLoop::new();

    let wb = glutin::window::WindowBuilder::new()
        .with_title("Image Viewer with Reclutch")
        .with_inner_size(
            glutin::dpi::PhysicalSize::new(window_size.0 as _, window_size.1 as _)
                .to_logical(event_loop.primary_monitor().hidpi_factor()),
        )
        .with_min_inner_size(
            glutin::dpi::PhysicalSize::new(400.0, 200.0)
                .to_logical(event_loop.primary_monitor().hidpi_factor()),
        );

    let context = glutin::ContextBuilder::new()
        //.with_vsync(true) // fast dragging motion at the cost of high GPU usage
        .build_windowed(wb, &event_loop)
        .unwrap();

    let context = unsafe { context.make_current().unwrap() };

    gl::load_with(|proc| context.get_proc_address(proc) as _);

    let mut display = display::skia::SkiaGraphicsDisplay::new_gl_framebuffer(
        &display::skia::SkiaOpenGlFramebuffer {
            framebuffer_id: 0,
            size: (window_size.0 as _, window_size.1 as _),
        },
    )
    .unwrap();

    display
        .push_command_group(&[DisplayCommand::Clear(Color::new(1.0, 1.0, 1.0, 1.0))])
        .unwrap();

    let mut latest_window_size = window_size;
    let mut global_q = RcEventQueue::new();

    let mut globals = Globals {
        hidpi_factor: context.window().hidpi_factor(),
        cursor: Point::default(),
        size: Size::new(window_size.0 as _, window_size.1 as _),
    };

    /*let mut panel = Panel::new(
        Point::new(20.0, 20.0),
        Size::new(378.4, 100.0),
        &mut global_q,
    );*/
    let mut panel_container = PanelContainer::new();

    panel_container.add_panel(Panel::new(
        Point::new(10.0, 10.0),
        Size::new(378.4, 100.0),
        "Reclutch Logo".into(),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), file!()))
            .parent()
            .unwrap()
            .join("../../../.media/reclutch.png")
            .canonicalize()
            .unwrap()
            //.to_str()
            //.unwrap()
            .into(),
        &mut global_q,
    ));

    panel_container.add_panel(Panel::new(
        Point::new(30.0, 30.0),
        Size::new(300.0, 200.0),
        "Photography (by S. Unrau)".into(),
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), file!()))
            .parent()
            .unwrap()
            .join("../../../reclutch/examples/image_viewer/image.jpg")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        &mut global_q,
    ));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                if display.size().0 != latest_window_size.0 as _
                    || display.size().1 != latest_window_size.1 as _
                {
                    display
                        .resize((latest_window_size.0 as _, latest_window_size.1 as _))
                        .unwrap();
                }

                panel_container.draw(&mut display);
                display.present(None).unwrap();
                context.swap_buffers().unwrap();
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                let position = position.to_physical(globals.hidpi_factor);
                globals.cursor = Point::new(position.x as _, position.y as _);
                global_q.push(GlobalEvent::MouseMove(globals.cursor.clone()));
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button: glutin::event::MouseButton::Left,
                        state,
                        ..
                    },
                ..
            } => match state {
                glutin::event::ElementState::Pressed => {
                    global_q.push(GlobalEvent::MouseClick(ConsumableEvent::new(
                        globals.cursor.clone(),
                    )));
                }
                glutin::event::ElementState::Released => {
                    global_q.push(GlobalEvent::MouseRelease(globals.cursor.clone()));
                }
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                let size = size.to_physical(context.window().hidpi_factor());
                latest_window_size = (size.width as _, size.height as _);
                globals.size.width = size.width as _;
                globals.size.height = size.height as _;
                global_q.push(GlobalEvent::WindowResize);
            }
            _ => return,
        }

        panel_container.update(&mut globals);
        context.window().request_redraw();
    });
}

#[cfg(not(feature = "skia"))]
fn main() {
    compile_error!("this example requires the Skia backend")
}