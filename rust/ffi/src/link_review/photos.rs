//! Original-slot transport for the shared frozen comparison thumbnail reader.
use super::{failed, link_network, LinkReviewError, LinkReviewSource, ProfileAdoptionReview};
use crate::preview::Rgba;

#[uniffi::export]
impl ProfileAdoptionReview {
    /// Decode one frozen source thumbnail off the UI thread; no publication, disk cache or choices.
    /// Stored valid bytes work offline. Network fallback uses only this review's admitted network.
    ///
    /// # Errors
    /// Returns changed original context, invalid reference, unavailable thumbnail or storage errors.
    pub fn photo_thumbnail(
        &self,
        source: LinkReviewSource,
        index: u32,
    ) -> Result<Rgba, LinkReviewError> {
        link_network(|runtime| {
            let admitted = self.owner.open()?;
            self.admit(&admitted)?;
            let index = usize::try_from(index).map_err(failed)?;
            let source = source.into();
            let cached = self
                .review
                .cached_thumbnail(admitted.session.store(), source, index)
                .map_err(failed)?;
            let decoded = match cached {
                Some(decoded) => decoded,
                None => runtime.block_on(async {
                    let mut nodes = crate::peer_photos::reachable_at(
                        &admitted.operation.slot().dir,
                        &self.network,
                    )
                    .await?;
                    self.review
                        .fetch_thumbnail(&mut nodes, source, index)
                        .await
                        .map_err(failed)
                })?,
            };
            self.admit(&admitted)?;
            Ok(Rgba {
                width: decoded.width,
                height: decoded.height,
                pixels: decoded.pixels,
            })
        })
    }
}
