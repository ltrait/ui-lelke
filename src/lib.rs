use gpui::{
    Action, App, Application, AsyncApp, Bounds, Context, Entity, Global, KeyBinding, Rgba,
    Subscription, Window, WindowBounds, WindowOptions, colors::Colors, div, prelude::*, px, size,
};

use tracing::{debug, error, info};

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::channel::oneshot;

use ltrait::{
    UI,
    color_eyre::eyre::{Result, bail, eyre},
    launcher::batcher::Batcher,
    ui::{Buffer, Position},
};

pub use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, SharedString,
    WindowBackgroundAppearance, rgba,
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
    pub font: Option<Font>,
}

impl From<Colors> for LelkeTheme {
    fn from(value: Colors) -> Self {
        Self {
            bg: value.background,
            border: value.border,
            entry_bg: value.container,
            entry_fg: value.text,
            selected_bg: value.disabled,
            selected_fg: value.selected,
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
        Colors::dark().into()
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

    fn register_callbacks(&self, cx: &mut App) {
        info!("Registering action callbacks");

        cx.on_action(actions::quit);
    }
}

pub mod actions {
    use gpui::{BorrowAppContext, actions};
    use tracing::{error, info};

    use crate::BatcherConnection;

    actions!(lelke, [MoveNext, MovePrevious, Quit, Select,]);

    pub(crate) fn quit(_: &Quit, cx: &mut gpui::App) {
        info!("Quitting...");

        cx.update_global::<BatcherConnection, _>(|bc, _| {
            bc.0.take().map(|tx| {
                if let Err(_) = tx.send(None) {
                    error!("failed to send none");
                }
            })
        });

        info!("Shutting down");
        cx.shutdown();
    }

    pub(crate) fn select(_: &Select, cx: &mut gpui::App) {
        info!("Quitting... with Selected");

        cx.update_global::<BatcherConnection, _>(|bc, _| {
            bc.0.take().map(|tx| {
                if let Err(_) = tx.send(Some(todo!())) {
                    // TODO:
                    error!("failed to send id");
                }
            })
        });

        info!("Shutting down");
        cx.shutdown();
    }
}

pub fn example_bindings(kbd: &mut KeyBinder) {
    use actions::*;

    kbd.bind("tab", MoveNext);
    kbd.bind("shift-tab", MovePrevious);
    kbd.bind("up", MoveNext); // TODO: いまのところは下入力想定
    kbd.bind("down", MovePrevious);
    kbd.bind("escape", Quit);
    kbd.bind("enter", Select);
}

async fn batcher_thread<Cushion>(
    mut cx: AsyncApp,
    mut batcher: Batcher<Cushion, LelkeEntry>,
    buf: Entity<Buffer<(LelkeEntry, usize)>>,
    mut rx: oneshot::Receiver<Option<usize>>,
    tx: oneshot::Sender<Option<Cushion>>,
) -> Result<()>
where
    Cushion: Send + Sync + 'static,
{
    let mut more = true;
    while more {
        let from = batcher.prepare().await;
        more = buf
            .update(&mut cx, |prev, cx| {
                let v = batcher.merge(prev, from);

                debug!("merged buffer length: {}", prev.len());

                cx.notify();

                v
            })
            .map_err(|err| eyre!("{err}"))??;

        if let Some(id) = rx.try_recv()? {
            info!("received id, now computing cushion");
            tx.send(id.map(|id| batcher.compute_cushion(id)).transpose()?)
                .map_err(|_| eyre!("failed to send cushion: receiver dropped"))?;
            return Ok(());
        }
    }

    let id = rx.await?;
    info!("received id, now computing cushion");
    tx.send(id.map(|id| batcher.compute_cushion(id)).transpose()?)
        .map_err(|_| eyre!("failed to send cushion: receiver dropped"))?;

    Ok(())
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
                    let mut _subscriptions = vec![];

                    theme.apply(win);

                    let buf: Entity<Buffer<(LelkeEntry, usize)>> = cx.new(|_| Buffer::default());
                    _subscriptions.push(cx.observe(&buf, |_, _| ()));

                    let (sen, rx) = oneshot::channel();

                    cx.set_global(BatcherConnection(Some(sen)));

                    binder.bind_keys(cx);
                    binder.register_callbacks(cx);

                    {
                        let buf = buf.clone();

                        cx.spawn(async move |cx| {
                            if let Err(e) = batcher_thread(cx.clone(), batcher, buf, rx, tx).await {
                                error!("Batcher thread of Lelke has been panicked: {e}");
                            }
                        })
                        .detach();
                    }

                    cx.new(|_| RootView {
                        buf,
                        _subscriptions,
                    })
                },
            )
            .unwrap();

            cx.activate(true);
        });

        info!("receiving Cushion");
        if let Ok(cu) = cushion.await {
            Ok(cu)
        } else {
            bail!("failed to receive cushion")
        }
    }
}

// begin gpui

struct RootView {
    buf: Entity<Buffer<(LelkeEntry, usize)>>,
    _subscriptions: Vec<Subscription>,
}

impl Render for RootView {
    #[tracing::instrument(skip_all)]
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.global::<LelkeConfig>();
        let theme = &config.theme;

        div()
            .child(HelloWorld {
                text: "World".into(),
            })
            .bg(theme.bg)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.entry_fg) // TODO: 違うかも
            .child(Items.compute_element(&self.buf, cx))
    }
}

struct HelloWorld {
    text: SharedString,
}

impl IntoElement for HelloWorld {
    type Element = gpui::Div;

    fn into_element(self) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .size(px(500.0))
            .justify_center()
            .items_center()
            .shadow_lg()
            .text_xl()
            .child(format!("Hello, {}!", &self.text))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::red())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::white()),
                    )
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::green())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::white()),
                    )
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::blue())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::white()),
                    )
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::yellow())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::white()),
                    )
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::black())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::white()),
                    )
                    .child(
                        div()
                            .size_8()
                            .bg(gpui::white())
                            .border_1()
                            .border_dashed()
                            .rounded_md()
                            .border_color(gpui::black()),
                    ),
            )
    }
}

struct Items;

impl Items {
    #[tracing::instrument(skip_all)]
    fn compute_element(
        &self,
        buf: &Entity<Buffer<(LelkeEntry, usize)>>,
        cx: &mut App,
    ) -> impl IntoElement {
        // ここではbufferの中身を見なならloopをまわしてLelkeEntry -> ElementにしてcacheしつつListにする
        let buf = buf.read(cx);
        let mut pos = Position::default();

        debug!("buffer length: {}", buf.len());
        while let Some((entry, _)) = buf.next(&mut pos) {
            debug!("entry, pos: {entry:?} {pos:?}");
            // TODO:
        }

        div()
    }
}
