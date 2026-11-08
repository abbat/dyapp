// A photo in a chat: the list row, the placeholder, the blur until Show or acceptance, expiry, an
// undecodable preview, the text actions a photo must not offer, the tap on the original and
// sending a photo without its metadata.
const checkChatPhoto = async () => {
  const fixture = await window.uiSmokeChatPhotoFixture();
  await window.uiSmokeChatPhotoRender(fixture);

  await window.uiSmokeChatPhotoSend(fixture);

  await window.uiSmokePhotoSettings();
};
// The profile editor: the status line for a bad file, adding, the limit at six, moving with focus
// kept, removal behind its dialog, the form busy state, and the photo line of the publish dialog.
const checkProfilePhotos = window.uiSmokeProfilePhotos;
// Other people's photos: lists ask only for thumbnails, the view for full images under the download
// setting, and a refused blob keeps the letters and is asked for again on the next view.
const checkPeerPhotos = window.uiSmokePeerPhotos;
const addRealPhoto = async () => {
  const source = document.createElement('canvas');
  source.width = 640;
  source.height = 640;
  source.getContext('2d').fillRect(0, 0, 640, 640);
  const jpeg = await new Promise((resolve) => {
    source.toBlob(resolve, 'image/jpeg');
  });
  await openArea('profile-area');
  await addProfilePhoto(new File([jpeg], 'photo.jpg', { type: 'image/jpeg' }));
  if (byId('profile-photos').children.length !== 1) {
    throw new Error(`the photo was not added: ${byId('profile-status').textContent}`);
  }
};
const shownSide = () => document.querySelector('#view-photos .main canvas')?.width ?? 0;
const photoSteps = {
  // Over a real node: the thumbnail on the card and in the view, then the full image.
  'photo-seen': async (peer) => {
    await liveFound(peer);
    await window.peerPhotosShown;
    const thumbnail = shownSide();
    if (!thumbnail) throw new Error(`no thumbnail in the view: ${byId('view-status').textContent}`);
    const load = byId('view-photo-load');
    if (!load.hidden) {
      load.click();
      await window.peerPhotoLoaded;
    }
    if (shownSide() !== 640) {
      throw new Error(`no full image, side ${shownSide()}: ${byId('view-status').textContent}`);
    }
    byId('view-back').click();
    await livePoll(
      () => document.querySelector(`#search-results [data-peer-id="${peer}"] .avatar canvas`),
      'the thumbnail on the card',
    );
  },
};
globalThis.uiSmokePhotos = Object.freeze({
  addRealPhoto,
  checkChatPhoto,
  checkProfilePhotos,
  checkPeerPhotos,
  photoSteps,
});
