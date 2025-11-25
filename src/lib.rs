use gpui::{
    Action, App, Application, AsyncApp, Bounds, Context, Entity, Global, KeyBinding, Keystroke,
    Rgba, Subscription, Window, WindowBounds, WindowOptions, colors::Colors, div,
    prelude::*, px, size,
};

pub use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, SharedString,
    WindowBackgroundAppearance, rgba,
};
use gpui_component::{
    Root,
    input::{Input, InputEvent, InputState},
    kbd::Kbd,
};

use std::{path::PathBuf, sync::Arc, time::Duration};

use futures::{
    StreamExt,
    channel::{mpsc, oneshot},
    future::{Either, select},
};
use futures_timer::Delay;
use tracing::{debug, error, info, info_span};

use ltrait::{
    UI,
    color_eyre::eyre::{Result, bail, eyre},
    launcher::batcher::Batcher,
    ui::{Buffer, Position},
};

use derive_more::Debug;

#[derive(Debug, Clone)]
pub struct LelkeConfig {
    pub height: f32,
    pub width: f32,
    pub app_id: Option<String>,
    #[debug("binding")]
    pub bindings: Arc<dyn Fn(&mut KeyBinder) + Sync + Send + 'static>,
    pub theme: LelkeTheme,
    pub placeholder: &'static str,
}

#[derive(Debug, Clone)]
pub struct LelkeTheme {
    pub background_appearance: WindowBackgroundAppearance,

    pub bg: Rgba,
    pub entry_bg: Rgba,
    pub entry_fg: Rgba,
    pub selected_bg: Rgba,
    pub selected_fg: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub font: Option<Font>,
}

impl From<Colors> for LelkeTheme {
    fn from(value: Colors) -> Self {
        Self {
            bg: value.background,
            border: value.border,
            entry_bg: value.container,
            entry_fg: value.text,
            selected_bg: value.selected,
            selected_fg: value.selected_text,
            accent: value.selected,
            background_appearance: gpui::WindowBackgroundAppearance::Opaque,
            font: None,
        }
    }
}

impl LelkeTheme {
    fn apply(&self, win: &mut Window) {
        win.set_background_appearance(self.background_appearance);
    }

    pub fn default_dark() -> Self {
        Self {
            bg: gpui::rgb(0x1a1b26),
            entry_bg: gpui::rgb(0x24283b),
            entry_fg: gpui::rgb(0xc0caf5),
            selected_bg: gpui::rgb(0x33467c),
            selected_fg: gpui::rgb(0xf0f0f0),
            border: gpui::rgb(0x2f334d),
            accent: gpui::rgb(0x7aa2f7),
            background_appearance: gpui::WindowBackgroundAppearance::Opaque,
            font: None,
        }
    }

    pub fn default_light() -> Self {
        Colors::light().into()
    }
}

impl Global for LelkeConfig {}

pub struct Lelke {
    config: LelkeConfig,
}

impl Lelke {
    pub fn new(config: LelkeConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug)]
pub struct LelkeEntry {
    pub ty: LelkeEntryType,
    pub style: LelkeEntryStyle,
}

#[derive(Debug)]
pub enum LelkeEntryType {
    Text {
        text: String,
        right_text: Option<String>,
    },
    TextWithIcon {
        icon: PathBuf,
        text: String,
        right_text: Option<String>,
    },
    #[cfg(feature = "dev")]
    Simple(usize),
}

#[derive(Debug, Default)]
pub struct LelkeEntryStyle {
    pub bg: Option<Rgba>,
    pub fg: Option<Rgba>,
    pub font: Option<Font>,
}

#[derive(Debug, Default)]
pub struct KeyBinder {
    binds: Vec<KeyBinding>,
}

impl KeyBinder {
    pub fn bind<A: Action>(&mut self, keystrokes: &str, action: A) {
        self.binds.push(KeyBinding::new(keystrokes, action, None))
    }

    fn bind_keys(&self, cx: &mut App) {
        info!("Binding keys");

        cx.bind_keys(self.binds.clone());
    }

    fn register_callbacks(&self, cx: &mut App, buf: Entity<Buf>, selecting: Entity<Selecting>) {
        info!("Registering action callbacks");

        cx.on_action(actions::quit);
        cx.on_action(actions::select(buf.clone(), selecting.clone()));
        cx.on_action(actions::move_next(buf.clone(), selecting.clone()));
        cx.on_action(actions::move_previous(selecting));
    }
}

type Buf = Buffer<(LelkeEntry, usize)>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Selecting(usize);

pub mod actions {
    use gpui::{App, BorrowAppContext, Entity, actions};
    use ltrait::ui::Position;
    use tracing::{error, info};

    use crate::{BatcherConnection, Buf, Selecting};

    actions!(lelke, [MoveNext, MovePrevious, Quit, Select,]);

    pub(crate) fn quit(_: &Quit, cx: &mut App) {
        info!("Quitting...");

        cx.update_global::<BatcherConnection, _>(|bc, _| {
            if let Some(Err(_)) = bc.0.take().map(|tx| tx.send(None)) {
                error!("failed to send none");
            }
        });

        info!("Shutting down");
        cx.shutdown();
    }

    pub(crate) fn select(
        buf: Entity<Buf>,
        selecting: Entity<Selecting>,
    ) -> impl Fn(&Select, &mut App) {
        move |_: &Select, cx: &mut App| {
            info!("Quitting... with Selected");

            let pos = selecting.read(cx).0;
            let mut pos = Position(pos);
            let id = buf.read(cx).next(&mut pos).map(|(_, id)| *id);

            cx.update_global::<BatcherConnection, _>(|bc, _| {
                if let Some(Err(_)) = bc.0.take().map(|tx| tx.send(id)) {
                    error!("failed to send none");
                }
            });

            info!("Shutting down"); // もしあれならshutdownをobserve_globalでやるようにすればいい
            cx.shutdown();
        }
    }

    pub(crate) fn move_next(
        buf: Entity<Buf>,
        selecting: Entity<Selecting>,
    ) -> impl Fn(&MoveNext, &mut App) {
        move |_, cx| {
            let len = buf.read(cx).len();

            selecting.update(cx, |sel, cx| {
                sel.0 = sel.0.saturating_add(1).min(len.saturating_sub(1));

                cx.notify();
            })
        }
    }

    pub(crate) fn move_previous(selecting: Entity<Selecting>) -> impl Fn(&MovePrevious, &mut App) {
        move |_, cx| {
            // todo
            selecting.update(cx, |sel, cx| {
                sel.0 = sel.0.saturating_sub(1);

                cx.notify();
            })
        }
    }
}

pub fn example_bindings(kbd: &mut KeyBinder) {
    use actions::*;

    kbd.bind("tab", MoveNext);
    kbd.bind("shift-tab", MovePrevious);
    kbd.bind("up", MovePrevious);
    kbd.bind("down", MoveNext);
    kbd.bind("escape", Quit);
    kbd.bind("enter", Select);
}

fn send_cushion<T: Send>(
    tx: oneshot::Sender<Option<T>>,
    id: Option<usize>,
    batcher: Batcher<T, LelkeEntry>,
) -> Result<()> {
    info!("received id, now computing cushion");

    tx.send(id.map(|id| batcher.compute_cushion(id)).transpose()?)
        .map_err(|_| eyre!("failed to send cushion: receiver dropped"))?;

    Ok(())
}

fn set_input<T: Send>(
    cx: &mut AsyncApp,
    buf: &Entity<Buf>,
    user_input: &Entity<SharedString>,
    selecting: &Entity<Selecting>,

    batcher: &mut Batcher<T, LelkeEntry>,
    more: &mut bool,
) -> Result<()> {
    let input = user_input
        .read_with(cx, |input, _| input.to_string())
        .map_err(|e| eyre!("{e}"))?;

    info!("input: {input}");

    buf.update(cx, |buf, cx| {
        batcher.input(buf, &input);
        cx.notify();
    })
    .map_err(|e| eyre!("{e}"))?;

    selecting
        .update(cx, |sel, cx| {
            sel.0 = 0;
            cx.notify();
        })
        .map_err(|e| eyre!("{e}"))?;

    *more = true;

    Ok(())
}

async fn batcher_thread<Cushion>(
    mut cx: AsyncApp,
    mut batcher: Batcher<Cushion, LelkeEntry>,
    buf: Entity<Buf>,
    mut rx: oneshot::Receiver<Option<usize>>,
    tx: oneshot::Sender<Option<Cushion>>,
    mut inrx: mpsc::Receiver<()>,
    user_input: Entity<SharedString>,
    selecting: Entity<Selecting>,
    collecting: Entity<bool>,
) -> Result<()>
where
    Cushion: Send + Sync + 'static,
{
    let mut more = true;
    loop {
        if let Ok(Some(())) = inrx.try_next() {
            set_input(
                &mut cx,
                &buf,
                &user_input,
                &selecting,
                &mut batcher,
                &mut more,
            )?;
        }

        if let Some(id) = rx.try_recv()? {
            return send_cushion(tx, id, batcher);
        }

        if more {
            let from = batcher.prepare().await;

            more = buf
                .update(&mut cx, |prev, cx| {
                    let v = batcher.merge(prev, from);

                    debug!("merged buffer length: {}", prev.len());

                    cx.notify();

                    v
                })
                .map_err(|err| eyre!("{err}"))??;

            collecting
                .update(&mut cx, |_, _| more)
                .map_err(|err| eyre!("{err}"))?;
        } else {
            match select(inrx.next(), &mut rx).await {
                Either::Left((Some(()), _)) => set_input(
                    &mut cx,
                    &buf,
                    &user_input,
                    &selecting,
                    &mut batcher,
                    &mut more,
                )?,
                Either::Right((id, _)) => {
                    return send_cushion(tx, id?, batcher);
                }
                _ => (),
            }
        }
    }
}

struct BatcherConnection(Option<oneshot::Sender<Option<usize>>>);

impl Global for BatcherConnection {}

impl<Cushion> UI<Cushion> for Lelke
where
    Cushion: Send + Sync + 'static,
{
    type Context = LelkeEntry;

    #[tracing::instrument(skip_all)]
    async fn run(&self, batcher: Batcher<Cushion, Self::Context>) -> Result<Option<Cushion>> {
        let config = self.config.clone();

        let (tx, cushion) = oneshot::channel();

        Application::new().run(move |cx: &mut App| {
            gpui_component::init(cx);

            cx.set_global(config);

            let config = cx.global::<LelkeConfig>();
            let bounds = Bounds::centered(None, size(px(config.width), px(config.height)), cx);

            let theme = config.theme.clone();

            let mut binder = KeyBinder::default();
            (config.bindings)(&mut binder);
            let binder = binder;

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    kind: gpui::WindowKind::Floating,
                    app_id: config.app_id.clone(),
                    ..Default::default()
                },
                |win, cx| {
                    theme.apply(win);

                    binder.bind_keys(cx);

                    win.on_window_should_close(cx, |_, cx| {
                        let _span = info_span!("Re-quit");
                        let _enter = _span.enter();

                        actions::quit(&actions::Quit, cx);

                        true
                    });

                    let view = cx.new(|cx| {
                        let mut _subscriptions = vec![];

                        let buf: Entity<Buf> = cx.new(|_| Buffer::default());
                        _subscriptions.push(cx.observe(&buf, |_, _, _| ()));

                        let (sen, rx) = oneshot::channel();

                        cx.set_global(BatcherConnection(Some(sen)));

                        let (mut intx, inrx) = mpsc::channel(256);
                        let user_input = cx.new(|_| SharedString::new(""));

                        let selecting = cx.new(|_| Selecting(0));
                        _subscriptions.push(cx.observe(&selecting, |_, _, _| ()));

                        let state_indicator = cx.new(StateIndicator::new);
                        let collecting = state_indicator.read_with(cx, |v, _| v.collecting.clone());

                        {
                            let buf = buf.clone();
                            let user_input = user_input.clone();
                            let selecting = selecting.clone();

                            cx.spawn(async move |_, cx| {
                                if let Err(e) = batcher_thread(
                                    cx.clone(),
                                    batcher,
                                    buf,
                                    rx,
                                    tx,
                                    inrx,
                                    user_input,
                                    selecting,
                                    collecting,
                                )
                                .await
                                {
                                    error!("Batcher thread of Lelke has been panicked: {e}");
                                }
                            })
                            .detach();
                        }

                        binder.register_callbacks(cx, buf.clone(), selecting.clone());

                        let items = Items::new(buf, selecting, cx);

                        let placeholder = cx.global::<LelkeConfig>().placeholder;
                        let input_state =
                            cx.new(|cx| InputState::new(win, cx).placeholder(placeholder));

                        _subscriptions.push(cx.subscribe_in(&input_state, win, {
                            let input_state = input_state.clone();
                            move |_: &mut RootView, _, ev: &InputEvent, _window, cx| {
                                if let InputEvent::Change = ev {
                                    let value = input_state.read(cx).value();
                                    user_input.update(cx, |prev, _| {
                                        *prev = value;

                                        let _ = intx.try_send(());
                                    });
                                    cx.notify()
                                }
                            }
                        }));

                        RootView {
                            state_indicator,
                            input_state,
                            items,
                            _subscriptions,
                        }
                    });

                    cx.new(|cx| Root::new(view, win, cx))
                },
            )
            .unwrap();

            cx.activate(true);
        });

        info!("receiving Cushion");

        let timeout = Delay::new(Duration::from_millis(500));
        match select(cushion, timeout).await {
            Either::Left((Ok(cu), _)) => Ok(cu),
            Either::Right(_) => bail!("failed to receive cushion: timed out"),
            _ => bail!("failed to receive cushion"),
        }
    }
}

// begin gpui

struct RootView {
    state_indicator: Entity<StateIndicator>,
    items: Entity<Items>,
    input_state: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl Render for RootView {
    #[tracing::instrument(skip_all)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.global::<LelkeConfig>();
        let theme = &config.theme;

        let select_kbd = Kbd::binding_for_action(&actions::Select, None, window)
            .unwrap_or(Kbd::new(Keystroke::parse("enter").unwrap()));
        let quit_kbd = Kbd::binding_for_action(&actions::Quit, None, window)
            .unwrap_or(Kbd::new(Keystroke::parse("escape").unwrap()));
        let next_kbd = Kbd::new(Keystroke::parse("tab").unwrap());
        let prev_kbd = Kbd::new(Keystroke::parse("shift-tab").unwrap());

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.bg)
            .text_color(theme.entry_fg)
            // input at top
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(Input::new(&self.input_state)),
            )
            // items list fills remaining space
            .child(div().flex_1().child(self.items.clone()))
            // compact footer
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(self.state_indicator.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(next_kbd)
                            .child(prev_kbd)
                            .child(select_kbd)
                            .child(quit_kbd),
                    ),
            )
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
struct Items {
    buf: Entity<Buf>,
    selecting: Entity<Selecting>,
}

impl Items {
    fn new(buf: Entity<Buf>, selecting: Entity<Selecting>, cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self { buf, selecting })
    }
}

impl Render for Items {
    #[tracing::instrument(skip_all)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.global::<LelkeConfig>();
        let theme = &config.theme;

        let buf = self.buf.read(cx);
        let selecting = self.selecting.read(cx).0;

        let mut pos = Position::default();
        let mut idx = 0usize;

        debug!("buffer length: {}", buf.len());

        let mut list = div().flex().flex_col().w_full();

        while let Some((entry, _)) = buf.next(&mut pos) {
            let is_selected = idx == selecting;

            let bg = if is_selected {
                theme.selected_bg
            } else {
                entry.style.bg.unwrap_or(theme.bg)
            };
            let fg = if is_selected {
                theme.selected_fg
            } else {
                entry.style.fg.unwrap_or(theme.entry_fg)
            };

            let label: SharedString = match &entry.ty {
                LelkeEntryType::Text { text, .. }
                | LelkeEntryType::TextWithIcon { text, .. } => text.clone().into(),
                #[cfg(feature = "dev")]
                LelkeEntryType::Simple(u) => format!("Simple({u})").into(),
            };

            let right: Option<SharedString> = match &entry.ty {
                LelkeEntryType::Text { right_text, .. }
                | LelkeEntryType::TextWithIcon { right_text, .. } => {
                    right_text.clone().map(Into::into)
                }
                #[cfg(feature = "dev")]
                LelkeEntryType::Simple(_) => None,
            };

            let icon_path: Option<PathBuf> = match &entry.ty {
                LelkeEntryType::TextWithIcon { icon, .. } => Some(icon.clone()),
                _ => None,
            };

            // accent left border for selected row
            let accent_bar = if is_selected {
                div().w(px(3.)).h(px(28.)).bg(theme.accent)
            } else {
                div().w(px(3.)).h(px(28.))
            };

            // left side: optional icon + label
            let mut left = div().flex().items_center().gap_2();

            if let Some(icon) = icon_path {
                left = left.child(
                    gpui::img(icon)
                        .w(px(20.))
                        .h(px(20.))
                        .flex_none(),
                );
            }

            left = left.child(label);

            // right side: right_text in muted color
            let mut content = div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
                .text_color(fg);

            content = content.child(left);

            if let Some(right) = right {
                content = content.child(
                    div()
                        .text_xs()
                        .text_color(theme.border)
                        .child(right),
                );
            }

            let row = div()
                .flex()
                .items_center()
                .w_full()
                .bg(bg)
                .child(accent_bar)
                .child(div().flex_1().child(content));

            list = list.child(row);

            idx += 1;
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .id("items-scroll")
            .overflow_y_scroll()
            .child(list)
    }
}

struct StateIndicator {
    collecting: Entity<bool>,
    _subscriptions: Vec<Subscription>,
}

impl StateIndicator {
    fn new(cx: &mut Context<'_, Self>) -> Self {
        let mut _subscriptions = vec![];

        let collecting = cx.new(|_| true);
        _subscriptions.push(cx.observe(&collecting, |_, _, _| ()));

        Self {
            collecting: collecting.clone(),
            _subscriptions,
        }
    }
}

impl Render for StateIndicator {
    #[tracing::instrument(skip_all)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collecting = *self.collecting.read(cx);
        let config = cx.global::<LelkeConfig>();
        let theme = &config.theme;

        // accent when collecting, muted when done
        let dot_color = if collecting {
            theme.accent
        } else {
            gpui::rgb(0x6b6b6b)
        };

        div()
            .flex()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(theme.border)
            .child(
                div()
                    .w(px(8.))
                    .h(px(8.))
                    .rounded_full()
                    .bg(dot_color),
            )
            .child(if collecting {
                "Loading"
            } else {
                "Ready"
            })
    }
}
