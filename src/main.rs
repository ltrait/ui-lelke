use ltrait::color_eyre::Result;

#[cfg(not(feature = "dev"))]
fn main() -> Result<()> {
    use ltrait::color_eyre::eyre::eyre;

    eyre!(
        "This crate cannot be executed without the `dev` feature flag. Please enable it or use as a lib crate"
    )?
}

#[cfg(feature = "dev")]
#[derive(Debug)]
enum Context {
    Simple(usize),
}

#[cfg(feature = "dev")]
#[tokio::main]
async fn main() -> Result<()> {
    use ltrait::{Launcher, Level, action::ClosureAction, source::from_iter};
    use ltrait_ui_lelke::{Lelke, LelkeEntry, LelkeEntryStyle, LelkeTheme};

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(Level::DEBUG)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::ACTIVE)
        .init();

    let launcher = Launcher::default()
        .add_raw_source(from_iter(vec![
            Context::Simple(0),
            Context::Simple(1),
            Context::Simple(2),
        ]))
        .set_ui(
            Lelke::new(ltrait_ui_lelke::LelkeConfig {
                height: 500.,
                width: 500.,
                app_id: Some("lelke".into()),
                theme: LelkeTheme::default_dark(),
            }),
            |Context::Simple(u)| LelkeEntry {
                ty: ltrait_ui_lelke::LelkeEntryType::Simple(*u),
                style: LelkeEntryStyle::default(),
            },
        )
        .add_raw_action(ClosureAction::new(|c| {
            println!("{c:?}");
            Ok(())
        }));

    launcher.run().await?;

    Ok(())
}
