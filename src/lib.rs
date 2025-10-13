use gpui::{
    App, Application, AsyncApp, Bounds, Context, Entity, Font, Global, Rgba, SharedString,
    Subscription, Window, WindowBounds, WindowOptions, colors::Colors, div, prelude::*, px, size,
};

use tracing::{debug, error};

use std::path::PathBuf;

use futures::channel::oneshot;

use ltrait::{
    UI,
    color_eyre::eyre::{Result, eyre},
    launcher::batcher::Batcher,
    ui::{Buffer, Position},
};

#[derive(Debug, Clone)]
pub struct LelkeConfig {
    pub height: f32,
    pub width: f32,
    pub app_id: Option<String>,
    pub theme: LelkeTheme,
}

#[derive(Debug, Clone)]
pub struct LelkeTheme {
    pub background_appearance: gpui::WindowBackgroundAppearance,

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

async fn batcher_thread<Cushion>(
    mut cx: AsyncApp,
    mut batcher: Batcher<Cushion, LelkeEntry>,
    buf: Entity<Buffer<(LelkeEntry, usize)>>,
    mut rx: oneshot::Receiver<usize>,
    tx: oneshot::Sender<Cushion>,
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
            tx.send(batcher.compute_cushion(id)?)
                .map_err(|_| eyre!("oneshot channel is full of cushion.??"))?;
            return Ok(());
        }
    }

    let id = rx.await?;
    tx.send(batcher.compute_cushion(id)?)
        .map_err(|_| eyre!("oneshot channel is full of cushion.??"))?;

    Ok(())
}

struct BatcherConnection<T>(oneshot::Sender<usize>, oneshot::Receiver<T>);

impl<T: 'static> Global for BatcherConnection<T> {}

impl<Cushion> UI<Cushion> for Lelke
where
    Cushion: Send + Sync + 'static,
{
    type Context = LelkeEntry;

    #[tracing::instrument(skip_all)]
    async fn run(&self, batcher: Batcher<Cushion, Self::Context>) -> Result<Option<Cushion>> {
        let config = self.config.clone();
        Application::new().run(move |cx: &mut App| {
            cx.set_global(config);

            let config = cx.global::<LelkeConfig>();
            let bounds = Bounds::centered(None, size(px(config.width), px(config.height)), cx);

            let theme = config.theme.clone();

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

                    let (tx, rec) = oneshot::channel();
                    let (sen, rx) = oneshot::channel();

                    cx.set_global(BatcherConnection(sen, rec));

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

        Ok(None)
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
            .bg(theme.bg.clone())
            .border_1()
            .border_color(theme.border.clone())
            .text_color(theme.entry_fg.clone()) // TODO: 違うかも
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
