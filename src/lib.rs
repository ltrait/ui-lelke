use ltrait::color_eyre::eyre::Result;
use ltrait::{
    UI,
    launcher::batcher::Batcher,
    ui::{Buffer, Position},
};

pub struct Lelke;
pub struct LelkeEntry;

impl<'a> UI<'a> for Lelke {
    type Context = LelkeEntry;

    async fn run<Cusion: 'a + Send>(
        &self,
        mut batcher: Batcher<'a, Cusion, Self::Context>,
    ) -> Result<Option<Cusion>> {
        // TODO: remove dummy impl
        let mut more = true;
        let mut buf: Buffer<(LelkeEntry, usize)> = Buffer::default();

        while more {
            let from = batcher.prepare().await;
            more = batcher.merge(&mut buf, from).await?;
        }

        let mut pos = Position::default();
        let mut least_one = false;
        while let Some(_) = buf.next(&mut pos) {
            if !least_one {
                least_one = true;
            }
        }

        if least_one {
            Ok(Some(batcher.compute_cusion(0)?))
        } else {
            Ok(None)
        }
    }
}

impl Lelke {
    pub fn new() -> Self {
        Self
    }
}
