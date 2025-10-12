use ltrait::color_eyre::Result;

#[cfg(not(feature = "dev"))]
fn main() -> Result<()> {
    eyre!(
        "This crate cannot be executed without the `dev` feature flag. Please enable it or use as a lib crate"
    );
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
    use ltrait_ui_lelke::{Lelke, LelkeEntry};

    let _guard = ltrait::setup(Level::INFO)?;

    let launcher = Launcher::default()
        .add_raw_source(from_iter(vec![
            Context::Simple(0),
            Context::Simple(1),
            Context::Simple(2),
        ]))
        .set_ui(Lelke::new(), |_| LelkeEntry {})
        .add_raw_action(ClosureAction::new(|c| {
            println!("{c:?}");
            Ok(())
        }));

    launcher.run().await?;

    Ok(())
}
